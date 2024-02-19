use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use crate::utxo::{UtxoContextBinding, UtxoContextId};
use crate::wasm::utxo::UtxoProcessor;
use crate::wasm::Balance;
use kaspa_addresses::IAddressArray;
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
        processor: UtxoProcessor;
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
/// When created, UtxoContext accepts {@link IUtxoContextArgs} interface that
/// can contain an optional id parameter.  If supplied, this `id` parameter
/// will be included in all notifications emitted by the UtxoContext as
/// well as included as a part of {@link ITransactionRecord} emitted when
/// transactions occur. If not provided, a random id will be generated. This id
/// typically represents an account id in the context of a wallet application.
///
/// UtxoContext maintains a real-time cumulative balance of all addresses
/// registered against it and provides balance update notification events
/// when the balance changes.
///
/// The UtxoContext balance is comprised of 3 values:
///     - `mature`: amount of funds available for spending.
///     - `pending`: amount of funds that are being received.
///     - `outgoing`: amount of funds that are being sent but are not yet accepted by the network.
/// Please see {@link IBalance} for more details.
///
/// UtxoContext can be supplied as a UTXO source to the transaction {@link Generator}
/// allowing the {@link Generator} to create transactions using the
/// UTXO entries it manages.
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
#[derive(Clone)]
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
}

#[wasm_bindgen]
impl UtxoContext {
    #[wasm_bindgen(constructor)]
    pub async fn ctor(js_value: IUtxoContextArgs) -> Result<UtxoContext> {
        let UtxoContextCreateArgs { processor, binding } = js_value.try_into()?;
        let inner = native::UtxoContext::new(processor.inner(), binding);
        Ok(UtxoContext { inner })
    }

    /// Performs a scan of the given addresses and registers them in the context for event notifications.
    #[wasm_bindgen(js_name = "trackAddresses")]
    pub async fn track_addresses(&self, addresses: IAddressArray, optional_current_daa_score: Option<BigInt>) -> Result<()> {
        let current_daa_score = if let Some(big_int) = optional_current_daa_score {
            Some(big_int.try_into().map_err(|v| Error::custom(format!("Unable to convert BigInt value {v:?}")))?)
        } else {
            None
        };
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner().scan_and_register_addresses(addresses, current_daa_score).await
    }

    /// Unregister a list of addresses from the context. This will stop tracking of these addresses.
    #[wasm_bindgen(js_name = "unregisterAddresses")]
    pub async fn unregister_addresses(&self, addresses: IAddressArray) -> Result<()> {
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner().unregister_addresses(addresses).await
    }

    /// Clear the UtxoContext.  Unregisters all addresses and clears all UTXO entries.
    pub async fn clear(&self) -> Result<()> {
        self.inner().clear().await
    }

    /// Returns all mature UTXO entries that are currently managed by the UtxoContext and are available for spending.
    pub fn mature(&self) -> Result<Array> {
        let context = self.context();
        let array = Array::new();
        for entry in context.mature.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array)
    }

    /// Returns pending UTXO entries that are currently managed by the UtxoContext.
    pub fn pending(&self) -> Result<Array> {
        let context = self.context();
        let array = Array::new();
        for (_, entry) in context.pending.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array)
    }

    /// Current {@link Balance} of the UtxoContext.
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> Option<Balance> {
        // self.inner().balance().map(Balance::from).map(JsValue::from).unwrap_or(JsValue::UNDEFINED)
        self.inner().balance().map(Balance::from)
    }

    // / Re-calculate the balance of the UtxoContext.
    // #[wasm_bindgen(js_name=updateBalance)]
    // pub async fn calculate_balance(&self) -> crate::wasm::Balance {
    //     self.inner.calculate_balance().await.into()
    // }
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

impl TryFrom<JsValue> for UtxoContext {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        Ok(ref_from_abi!(UtxoContext, &value)?)
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
            let processor = object.get::<UtxoProcessor>("processor")?;

            let binding = if let Some(id) = object.try_get::<Hash>("id")? {
                UtxoContextBinding::Id(UtxoContextId::new(id))
            } else {
                UtxoContextBinding::default()
            };

            Ok(UtxoContextCreateArgs { binding, processor })
        } else {
            Err(Error::custom("UtxoProcessor: supplied value must be an object"))
        }
    }
}
