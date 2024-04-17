use crate::imports::*;
use crate::result::Result;
use crate::tx::{generator as native, Fees, PaymentDestination, PaymentOutputs};
use crate::utxo::{TryIntoUtxoEntryReferences, UtxoEntryReference};
use crate::wasm::tx::generator::*;
use crate::wasm::tx::IFees;
// use crate::wasm::wallet::Account;
use crate::wasm::UtxoContext;

// TODO-WASM fix outputs
#[wasm_bindgen(typescript_custom_section)]
const TS_GENERATOR_SETTINGS_OBJECT: &'static str = r#"
/**
 * Configuration for the transaction {@link Generator}. This interface
 * allows you to specify UTXO sources, transaction outputs, change address,
 * priority fee, and other transaction parameters.
 * 
 * If the total number of UTXOs needed to satisfy the transaction outputs
 * exceeds maximum allowed number of UTXOs per transaction (limited by
 * the maximum transaction mass), the {@link Generator} will produce 
 * multiple chained transactions to the change address and then used these
 * transactions as a source for the "final" transaction.
 * 
 * @see 
 *      {@link kaspaToSompi},
 *      {@link Generator}, 
 *      {@link PendingTransaction}, 
 *      {@link UtxoContext}, 
 *      {@link UtxoEntry},
 *      {@link createTransactions},
 *      {@link estimateTransactions}
 * @category Wallet SDK
 */
interface IGeneratorSettingsObject {
    /** 
     * Final transaction outputs (do not supply change transaction).
     * 
     * Typical usage: { address: "kaspa:...", amount: 1000n }
     */
    outputs: PaymentOutput | IPaymentOutput[];
    /** 
     * Address to be used for change, if any. 
     */
    changeAddress: Address | string;
    /** 
     * Priority fee in SOMPI.
     * 
     * If supplying `bigint` value, it will be interpreted as a sender-pays fee.
     * Alternatively you can supply an object with `amount` and `source` properties
     * where `source` contains the {@link FeeSource} enum.
     * 
     * **IMPORTANT:* When sending an outbound transaction (transaction that
     * contains outputs), the `priorityFee` must be set, even if it is zero.
     * However, if the transaction is missing outputs (and thus you are
     * creating a compound transaction against your change address),
     * `priorityFee` should not be set (i.e. it should be `undefined`).
     * 
     * @see {@link IFees}, {@link FeeSource}
     */
    priorityFee?: IFees | bigint;
    /**
     * UTXO entries to be used for the transaction. This can be an
     * array of UtxoEntry instances, objects matching {@link IUtxoEntry}
     * interface, or a {@link UtxoContext} instance.
     */
    entries: IUtxoEntry[] | UtxoEntryReference[] | UtxoContext;
    /**
     * Optional number of signature operations in the transaction.
     */
    sigOpCount?: number;
    /**
     * Optional minimum number of signatures required for the transaction.
     */
    minimumSignatures?: number;
    /**
     * Optional data payload to be included in the transaction.
     */
    payload?: Uint8Array | HexString;

    /**
     * Optional NetworkId or network id as string (i.e. `mainnet` or `testnet-11`). Required when {@link IGeneratorSettingsObject.entries} is array
     */
    networkId?: NetworkId | string
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Object, typescript_type = "IGeneratorSettingsObject")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type IGeneratorSettingsObject;
}

/// Generator is a type capable of generating transactions based on a supplied
/// set of UTXO entries or a UTXO entry producer (such as {@link UtxoContext}). The Generator
/// accumulates UTXO entries until it can generate a transaction that meets the
/// requested amount or until the total mass of created inputs exceeds the allowed
/// transaction mass, at which point it will produce a compound transaction by forwarding
/// all selected UTXO entries to the supplied change address and prepare to start generating
/// a new transaction.  Such sequence of daisy-chained transactions is known as a "batch".
/// Each compound transaction results in a new UTXO, which is immediately reused in the
/// subsequent transaction.
///
/// The Generator constructor accepts a single {@link IGeneratorSettingsObject} object.
///
/// ```javascript
///
/// let generator = new Generator({
///     utxoEntries : [...],
///     changeAddress : "kaspa:...",
///     outputs : [
///         { amount : kaspaToSompi(10.0), address: "kaspa:..."},
///         { amount : kaspaToSompi(20.0), address: "kaspa:..."},
///         ...
///     ],
///     priorityFee : 1000n,
/// });
///
/// let pendingTransaction;
/// while(pendingTransaction = await generator.next()) {
///     await pendingTransaction.sign(privateKeys);
///     await pendingTransaction.submit(rpc);
/// }
///
/// let summary = generator.summary();
/// console.log(summary);
///
/// ```
/// @see
///     {@link IGeneratorSettingsObject},
///     {@link PendingTransaction},
///     {@link UtxoContext},
///     {@link createTransactions},
///     {@link estimateTransactions},
/// @category Wallet SDK
#[wasm_bindgen]
pub struct Generator {
    inner: Arc<native::Generator>,
}

#[wasm_bindgen]
impl Generator {
    #[wasm_bindgen(constructor)]
    pub fn ctor(args: IGeneratorSettingsObject) -> Result<Generator> {
        let settings = GeneratorSettings::try_from(args)?;

        let GeneratorSettings {
            network_id,
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
            GeneratorSource::UtxoEntries(utxo_entries) => {
                let change_address = change_address
                    .ok_or_else(|| Error::custom("changeAddress is required for Generator constructor with UTXO entries"))?;

                let network_id =
                    network_id.ok_or_else(|| Error::custom("networkId is required for Generator constructor with UTXO entries"))?;

                native::GeneratorSettings::try_new_with_iterator(
                    network_id,
                    Box::new(utxo_entries.into_iter()),
                    change_address,
                    sig_op_count,
                    minimum_signatures,
                    final_transaction_destination,
                    final_priority_fee,
                    payload,
                    multiplexer,
                )?
            }
            GeneratorSource::UtxoContext(utxo_context) => {
                let change_address = change_address
                    .ok_or_else(|| Error::custom("changeAddress is required for Generator constructor with UTXO entries"))?;

                native::GeneratorSettings::try_new_with_context(
                    utxo_context.into(),
                    change_address,
                    sig_op_count,
                    minimum_signatures,
                    final_transaction_destination,
                    final_priority_fee,
                    payload,
                    multiplexer,
                )?
            } // GeneratorSource::Account(account) => {
              //     let account: Arc<dyn crate::account::Account> = account.into();
              //     native::GeneratorSettings::try_new_with_account(account, final_transaction_destination, final_priority_fee, None)?
              // }
        };

        let abortable = Abortable::default();
        let generator = native::Generator::try_new(settings, None, Some(&abortable))?;

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
    UtxoContext(UtxoContext),
    // #[cfg(any(feature = "wasm32-sdk"), not(target_arch = "wasm32"))]
    // Account(Account),
}

/// Converts [`IGeneratorSettingsObject`] to a series of properties intended for use by the [`Generator`].
struct GeneratorSettings {
    pub network_id: Option<NetworkId>,
    pub source: GeneratorSource,
    pub multiplexer: Option<Multiplexer<Box<Events>>>,
    pub final_transaction_destination: PaymentDestination,
    pub change_address: Option<Address>,
    pub final_priority_fee: Fees,
    pub sig_op_count: u8,
    pub minimum_signatures: u16,
    pub payload: Option<Vec<u8>>,
}

impl TryFrom<IGeneratorSettingsObject> for GeneratorSettings {
    type Error = Error;
    fn try_from(args: IGeneratorSettingsObject) -> std::result::Result<Self, Self::Error> {
        let network_id = args.try_get::<NetworkId>("networkId")?;

        // lack of outputs results in a sweep transaction compounding utxos into the change address
        let outputs = args.get_value("outputs")?;
        let final_transaction_destination: PaymentDestination =
            if outputs.is_undefined() { PaymentDestination::Change } else { PaymentOutputs::try_owned_from(outputs)?.into() };

        let change_address = args.try_get_cast::<Address>("changeAddress")?.map(Cast::into_owned);

        let final_priority_fee = args.get::<IFees>("priorityFee")?.try_into()?;

        let generator_source = if let Ok(Some(context)) = args.try_get_cast::<UtxoContext>("entries") {
            GeneratorSource::UtxoContext(context.into_owned())
        } else if let Some(utxo_entries) = args.try_get_value("entries")? {
            GeneratorSource::UtxoEntries(utxo_entries.try_into_utxo_entry_references()?)
        } else {
            return Err(Error::custom("'entries', 'context' or 'account' property is required for Generator"));
        };

        let sig_op_count = args.get_value("sigOpCount")?;
        let sig_op_count =
            if !sig_op_count.is_undefined() { sig_op_count.as_f64().expect("sigOpCount should be a number") as u8 } else { 1 };

        let minimum_signatures = args.get_value("minimumSignatures")?;
        let minimum_signatures = if !minimum_signatures.is_undefined() {
            minimum_signatures.as_f64().expect("minimumSignatures should be a number") as u16
        } else {
            1
        };

        let payload = args.get_vec_u8("payload").ok();

        let settings = GeneratorSettings {
            network_id,
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
