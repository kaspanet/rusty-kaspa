use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use crate::utxo::{UtxoContextBinding, UtxoContextId};
use crate::wasm::utxo::UtxoProcessor;
use crate::wasm::{Balance, BalanceStrings};
use kaspa_addresses::AddressOrStringArrayT;
use kaspa_consensus_client::UtxoEntryReferenceArrayT;
use kaspa_hashes::Hash;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;

declare! {
    IUtxoContextArgs,
    r#"
    /**
     * UtxoContext constructor arguments.
     * 
     * @see {@link UtxoProcessor}, {@link UtxoContext}, {@link RpcClient}
     * @category Wallet SDK
     */
    export interface IUtxoContextArgs {
        /**
         * Associated UtxoProcessor.
         */
        processor: UtxoProcessor;
        /**
         * Optional id for the UtxoContext.
         * **The id must be a valid 32-byte hex string.**
         * You can use {@link sha256FromBinary} or {@link sha256FromText} to generate a valid id.
         * 
         * If not provided, a random id will be generated.
         * The IDs are deterministic, based on the order UtxoContexts are created.
         */
        id?: HexString;
    }
    "#,
}

///
/// UtxoContext is a class that provides a way to track addresses activity
/// on the Kaspa network.  When an address is registered with UtxoContext
/// it aggregates all UTXO entries for that address and emits events when
/// any activity against these addresses occurs.
///
/// UtxoContext constructor accepts {@link IUtxoContextArgs} interface that
/// can contain an optional id parameter.  If supplied, this `id` parameter
/// will be included in all notifications emitted by the UtxoContext as
/// well as included as a part of {@link ITransactionRecord} emitted when
/// transactions occur. If not provided, a random id will be generated. This id
/// typically represents an account id in the context of a wallet application.
/// The integrated Wallet API uses UtxoContext to represent wallet accounts.
///
/// **Exchanges:** if you are building an exchange wallet, it is recommended
/// to use UtxoContext for each user account.  This way you can track and isolate
/// each user activity (use address set, balances, transaction records).
///
/// UtxoContext maintains a real-time cumulative balance of all addresses
/// registered against it and provides balance update notification events
/// when the balance changes.
///
/// The UtxoContext balance is comprised of 3 values:
/// - `mature`: amount of funds available for spending.
/// - `pending`: amount of funds that are being received.
/// - `outgoing`: amount of funds that are being sent but are not yet accepted by the network.
///
/// Please see {@link IBalance} interface for more details.
///
/// UtxoContext can be supplied as a UTXO source to the transaction {@link Generator}
/// allowing the {@link Generator} to create transactions using the
/// UTXO entries it manages.
///
/// **IMPORTANT:** UtxoContext is meant to represent a single account.  It is not
/// designed to be used as a global UTXO manager for all addresses in a very large
/// wallet (such as an exchange wallet). For such use cases, it is recommended to
/// perform manual UTXO management by subscribing to UTXO notifications using
/// {@link RpcClient.subscribeUtxosChanged} and {@link RpcClient.getUtxosByAddresses}.
///
/// @see {@link IUtxoContextArgs},
/// {@link UtxoProcessor},
/// {@link Generator},
/// {@link createTransactions},
/// {@link IBalance},
/// {@link IBalanceEvent},
/// {@link IPendingEvent},
/// {@link IReorgEvent},
/// {@link IStasisEvent},
/// {@link IMaturityEvent},
/// {@link IDiscoveryEvent},
/// {@link IBalanceEvent},
/// {@link ITransactionRecord}
///
/// @category Wallet SDK
///
#[derive(Clone, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct UtxoContext {
    inner: native::UtxoContext,
}

impl UtxoContext {
    pub fn inner(&self) -> &native::UtxoContext {
        &self.inner
    }

    pub fn context(&self) -> MutexGuard<native::context::Context> {
        self.inner.context()
    }

    pub fn processor(&self) -> &native::UtxoProcessor {
        self.inner.processor()
    }
}

#[wasm_bindgen]
impl UtxoContext {
    #[wasm_bindgen(constructor)]
    pub fn ctor(js_value: IUtxoContextArgs) -> Result<UtxoContext> {
        let UtxoContextCreateArgs { processor, binding } = js_value.try_into()?;
        let inner = native::UtxoContext::new(processor.processor(), binding);
        Ok(UtxoContext { inner })
    }

    /// Performs a scan of the given addresses and registers them in the context for event notifications.
    #[wasm_bindgen(js_name = "trackAddresses")]
    pub async fn track_addresses(&self, addresses: AddressOrStringArrayT, optional_current_daa_score: Option<BigInt>) -> Result<()> {
        let current_daa_score = if let Some(big_int) = optional_current_daa_score {
            Some(big_int.try_into().map_err(|v| Error::custom(format!("Unable to convert BigInt value {v:?}")))?)
        } else {
            None
        };
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner().scan_and_register_addresses(addresses, current_daa_score).await?;
        Ok(())
    }

    /// Unregister a list of addresses from the context. This will stop tracking of these addresses.
    #[wasm_bindgen(js_name = "unregisterAddresses")]
    pub async fn unregister_addresses(&self, addresses: AddressOrStringArrayT) -> Result<()> {
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner().unregister_addresses(addresses).await
    }

    /// Clear the UtxoContext.  Unregister all addresses and clear all UTXO entries.
    /// IMPORTANT: This function must be manually called when disconnecting or re-connecting to the node
    /// (followed by address re-registration).  
    pub async fn clear(&self) -> Result<()> {
        self.inner().clear().await
    }

    pub fn active(&self) -> bool {
        let processor = self.inner().processor();
        processor.try_rpc_ctl().map(|ctl| ctl.is_connected()).unwrap_or(false) && processor.is_connected() && processor.is_running()
    }

    // Returns all mature UTXO entries that are currently managed by the UtxoContext and are available for spending.
    // This function is for informational purposes only.
    // pub fn mature(&self) -> Result<IUtxoEntryReferenceArray> {
    //     let context = self.context();
    //     let array = Array::new();
    //     for entry in context.mature.iter() {
    //         array.push(&JsValue::from(entry.clone()));
    //     }
    //     Ok(array.unchecked_into())
    // }

    ///
    /// Returns a range of mature UTXO entries that are currently
    /// managed by the UtxoContext and are available for spending.
    ///
    /// NOTE: This function is provided for informational purposes only.
    /// **You should not manage UTXO entries manually if they are owned by UtxoContext.**
    ///
    /// The resulting range may be less than requested if UTXO entries
    /// have been spent asynchronously by UtxoContext or by other means
    /// (i.e. UtxoContext has received notification from the network that
    /// UtxoEntries have been spent externally).
    ///
    /// UtxoEntries are kept in in the ascending sorted order by their amount.
    ///
    #[wasm_bindgen(js_name = "getMatureRange")]
    pub fn mature_range(&self, mut from: usize, mut to: usize) -> Result<UtxoEntryReferenceArrayT> {
        let context = self.context();
        if from > to {
            return Err(Error::custom("'from' must be less than or equal to 'to'"));
        }
        if from > context.mature.len() {
            from = context.mature.len();
        }
        if to > context.mature.len() {
            to = context.mature.len();
        }
        if from == to {
            return Ok(Array::new().unchecked_into());
        }
        let slice = context.mature.get(from..to).unwrap();
        let array = Array::new();
        for entry in slice.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array.unchecked_into())
    }

    /// Obtain the length of the mature UTXO entries that are currently
    /// managed by the UtxoContext.
    #[wasm_bindgen(getter, js_name = "matureLength")]
    pub fn mature_length(&self) -> usize {
        self.context().mature.len()
    }

    /// Returns pending UTXO entries that are currently managed by the UtxoContext.
    #[wasm_bindgen(js_name = "getPending")]
    pub fn pending(&self) -> Result<UtxoEntryReferenceArrayT> {
        let context = self.context();
        let array = Array::new();
        for (_, entry) in context.pending.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array.unchecked_into())
    }

    /// Current {@link Balance} of the UtxoContext.
    #[wasm_bindgen(getter, js_name = "balance")]
    pub fn balance(&self) -> Option<Balance> {
        self.inner().balance().map(Balance::from)
    }

    /// Current {@link BalanceStrings} of the UtxoContext.
    #[wasm_bindgen(getter, js_name = "balanceStrings")]
    pub fn balance_strings(&self) -> Result<Option<BalanceStrings>> {
        let network_id = self.inner.processor().network_id().ok();
        if let (Some(network_id), Some(balance)) = (network_id, self.inner().balance()) {
            let balance_strings = balance.to_balance_strings(&network_id.into(), None);
            Ok(Some(BalanceStrings::from(balance_strings)))
        } else {
            Ok(None)
        }
    }
}

impl From<native::UtxoContext> for UtxoContext {
    fn from(inner: native::UtxoContext) -> Self {
        Self { inner }
    }
}

impl From<UtxoContext> for native::UtxoContext {
    fn from(utxo_context: UtxoContext) -> Self {
        utxo_context.inner
    }
}

impl TryCastFromJs for UtxoContext {
    type Error = Error;
    fn try_cast_from(value: impl AsRef<JsValue>) -> Result<Cast<Self>, Self::Error> {
        Ok(Self::try_ref_from_js_value_as_cast(value)?)
    }
}

pub struct UtxoContextCreateArgs {
    processor: UtxoProcessor,
    binding: UtxoContextBinding,
}

impl TryFrom<IUtxoContextArgs> for UtxoContextCreateArgs {
    type Error = Error;
    fn try_from(value: IUtxoContextArgs) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let processor = object.get_cast::<UtxoProcessor>("processor")?;

            let binding = if let Some(id) = object.try_get_cast::<Hash>("id")? {
                UtxoContextBinding::Id(UtxoContextId::new(id.into_owned()))
            } else {
                UtxoContextBinding::default()
            };

            Ok(UtxoContextCreateArgs { binding, processor: processor.into_owned() })
        } else {
            Err(Error::custom("UtxoProcessor: supplied value must be an object"))
        }
    }
}
