use crate::imports::*;
use crate::result::Result;
use crate::runtime;
use crate::tx::{generator as native, Fees, PaymentDestination, PaymentOutputs};
use crate::utxo::UtxoEntryReference;
use crate::wasm::tx::generator::*;
use crate::wasm::wallet::Account;

#[wasm_bindgen]
extern "C" {
    /// Supports the following properties (all values must be supplied in SOMPI):
    /// - `outputs`: instance of [`PaymentOutputs`] or `[ [amount, address], [amount, address], ... ]`
    /// - `changeAddress`: [`Address`] or String representation of an address
    /// - `priorityFee`: BigInt or [`Fees`]
    /// - `utxoEntries`: Array of [`UtxoEntryReference`]
    /// - `sigOpCount`: [`u8`]
    /// - `minimumSignatures`: [`u16`]
    /// - `payload`: [`Uint8Array`] or hex String representation of a payload
    #[wasm_bindgen(extends = Object, is_type_of = Array::is_array, typescript_type = "PrivateKey[]")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type GeneratorSettingsObject;
}

/// [`Generator`] is a type capable of generating transactions based on a supplied
/// set of UTXO entries or a UTXO entry producer (such as `UtxoContext`). The [`Generator`]
/// accumulates UTXO entries until it can generate a transaction that meets the
/// requested amount or until the total mass of created inputs exceeds the allowed
/// transaction mass, at which point it will produce a compound transaction by forwarding
/// all selected UTXO entries to the supplied change address and prepare to start generating
/// a new transaction.  Such sequence of daisy-chained transactions is known as a "batch".
/// Each compount transaction results in a new UTXO, which is immediately reused in the
/// subsequent transaction.
///
/// ```javascript
///
/// let generator = await Generator.new({
///     utxoEntries : [...],
///     changeAddress : "kaspa:...",
///     outputs : [[1000, "kaspa:..."], [2000, "kaspa:..."], ...],
///     priorityFee : 1000n,
/// });
///
/// while(transaction = await generator.next()) {
///     await transaction.sign(privateKeys);
///     await transaction.submit(rpc);
/// }
///
/// let summary = generator.summary();
/// console.log(summary);
///
/// ```
///
#[wasm_bindgen]
pub struct Generator {
    inner: Arc<native::Generator>,
}

#[wasm_bindgen]
impl Generator {
    #[wasm_bindgen(constructor)]
    pub async fn js_new(args: GeneratorSettingsObject) -> Result<Generator> {
        let settings = GeneratorSettings::try_from(args)?;

        let GeneratorSettings {
            source,
            multiplexer,
            final_transaction_destination,
            change_address,
            final_priority_fee,
            sig_op_count,
            minimum_signatures,
            payload,
        } = settings;

        let settings = match source {
            GeneratorSource::UtxoEntries(utxo_entries) => native::GeneratorSettings::try_new_with_iterator(
                Box::new(utxo_entries.into_iter()),
                multiplexer,
                sig_op_count,
                minimum_signatures,
                change_address,
                final_transaction_destination,
                final_priority_fee,
                payload,
            )?,
            GeneratorSource::Account(account) => {
                let account: Arc<runtime::Account> = account.into();
                native::GeneratorSettings::try_new_with_account(&account, final_transaction_destination, final_priority_fee, None)
                    .await?
            }
        };

        let abortable = Abortable::default();
        let generator = native::Generator::new(settings, None, &abortable);

        Ok(Self { inner: Arc::new(generator) })
    }

    /// Generate next transaction
    pub async fn next(&self) -> Result<JsValue> {
        if let Some(transaction) = self.inner.generate_transaction().transpose() {
            let transaction = PendingTransaction::from(transaction?);
            Ok(transaction.into())
        } else {
            Ok(JsValue::NULL)
        }
    }

    pub async fn estimate(&self) -> Result<GeneratorSummary> {
        let mut stream = self.inner.stream();
        while stream.try_next().await?.is_some() {}
        Ok(self.summary())
    }

    pub fn summary(&self) -> GeneratorSummary {
        self.inner.summary().into()
    }
}

impl Generator {
    pub fn iter(&self) -> impl Iterator<Item = Result<native::PendingTransaction>> {
        self.inner.iter()
    }

    pub fn stream(&self) -> impl Stream<Item = Result<native::PendingTransaction>> {
        self.inner.stream()
    }
}

enum GeneratorSource {
    UtxoEntries(Vec<UtxoEntryReference>),
    Account(Account),
}

/// Converts [`GeneratorSettingsObject`] to a series of properties intended for use by the [`Generator`].
struct GeneratorSettings {
    pub source: GeneratorSource,
    pub multiplexer: Option<Multiplexer<Events>>,
    pub final_transaction_destination: PaymentDestination,
    pub change_address: Address,
    pub final_priority_fee: Fees,
    pub sig_op_count: u8,
    pub minimum_signatures: u16,
    pub payload: Option<Vec<u8>>,
}

impl TryFrom<GeneratorSettingsObject> for GeneratorSettings {
    type Error = Error;
    fn try_from(args: GeneratorSettingsObject) -> std::result::Result<Self, Self::Error> {
        // lack of outputs results in a sweep transaction compounding utxos into the change address
        let outputs = args.get("outputs")?;
        let final_transaction_destination: PaymentDestination =
            if outputs.is_undefined() { PaymentDestination::Change } else { PaymentOutputs::try_from(outputs)?.into() };

        let change_address = args.get("changeAddress")?;
        if change_address.is_undefined() {
            return Err(Error::custom("changeAddress is required"));
        }
        let change_address = Address::try_from(change_address)?;

        let final_priority_fee = Fees::try_from(args.get("priorityFee")?)?;

        let utxo_entries = args.get("utxoEntries")?;
        let generator_source = if !utxo_entries.is_undefined() {
            if let Some(utxo_entries) = utxo_entries.dyn_ref::<js_sys::Array>() {
                GeneratorSource::UtxoEntries(
                    utxo_entries.to_vec().iter().map(UtxoEntryReference::try_from).collect::<Result<Vec<_>, _>>()?,
                )
            } else {
                return Err(Error::custom("utxo_entries must be an array"));
            }
        } else {
            let account = args.get("account")?;
            if account.is_undefined() {
                return Err(Error::custom("'account' or 'utxoEntries' property is required for Generator"));
            } else {
                GeneratorSource::Account(ref_from_abi!(Account, &account)?)
            }
        };

        let sig_op_count = args.get("sigOpCount")?;
        let sig_op_count =
            if !sig_op_count.is_undefined() { sig_op_count.as_f64().expect("sigOpCount should be a number") as u8 } else { 1 };

        let minimum_signatures = args.get("minimumSignatures")?;
        let minimum_signatures = if !minimum_signatures.is_undefined() {
            minimum_signatures.as_f64().expect("minimumSignatures should be a number") as u16
        } else {
            1
        };

        let payload = args.get_vec_u8("payload").ok();

        let settings = GeneratorSettings {
            source: generator_source,
            multiplexer: None,
            final_transaction_destination,
            change_address,
            final_priority_fee,
            sig_op_count,
            minimum_signatures,
            payload,
        };

        Ok(settings)
    }
}
