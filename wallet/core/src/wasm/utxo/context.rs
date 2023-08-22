use crate::imports::*;
use crate::result::Result;
use crate::utxo as native;
use crate::utxo::{UtxoContextBinding, UtxoContextId};
use crate::wasm::utxo::UtxoProcessor;
use crate::wasm::Balance;
use kaspa_addresses::AddressList;
use kaspa_hashes::Hash;

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
    pub async fn ctor(js_value: JsValue) -> Result<UtxoContext> {
        let UtxoContextCreateArgs { processor, binding } = js_value.try_into()?;
        let inner = native::UtxoContext::new(processor.inner(), binding);
        Ok(UtxoContext { inner })
    }

    /// Performs a scan of the given addresses and registers them in the context for event notifications.
    #[wasm_bindgen(js_name = "trackAddresses")]
    pub async fn track_addresses(&self, addresses: JsValue, optional_current_daa_score: JsValue) -> Result<()> {
        let current_daa_score =
            if optional_current_daa_score.is_falsy() { None } else { optional_current_daa_score.try_as_u64().ok() };

        let addresses: Vec<Address> = AddressList::try_from(addresses)?.into();
        self.inner().scan_and_register_addresses(addresses, current_daa_score).await
    }

    /// Unregister a list of addresses from the context. This will stop tracking of these addresses.
    #[wasm_bindgen(js_name = "unregisterAddresses")]
    pub async fn unregister_addresses(&self, addresses: JsValue) -> Result<()> {
        let addresses: Vec<Address> = AddressList::try_from(addresses)?.into();
        self.inner().unregister_addresses(addresses).await
    }

    /// Clear the UtxoContext.  Unregisters all addresses and clears all UTXO entries.
    pub async fn clear(&self) -> Result<()> {
        self.inner().clear().await
    }

    /// Returns the mature UTXO entries that are currently in the context.
    pub fn mature(&self) -> Result<Array> {
        let context = self.context();
        let array = Array::new();
        for entry in context.mature.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array)
    }

    /// Returns the mature UTXO entries that are currently in the context.
    pub fn pending(&self) -> Result<Array> {
        let context = self.context();
        let array = Array::new();
        for (_, entry) in context.pending.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array)
    }

    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> JsValue {
        self.inner().balance().map(Balance::from).map(JsValue::from).unwrap_or(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name=updateBalance)]
    pub async fn calculate_balance(&self) -> crate::wasm::Balance {
        self.inner.calculate_balance().await.into()
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

impl TryFrom<JsValue> for UtxoContextCreateArgs {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let processor = object.get::<UtxoProcessor>("processor")?;

            let binding = if let Some(id) = object.try_get::<Hash>("id")? {
                UtxoContextBinding::Id(UtxoContextId::new(id))
            } else {
                UtxoContextBinding::default()
            };

            Ok(UtxoContextCreateArgs { binding, processor })
        } else {
            Err(Error::custom("UtxoProcessor: suppliedd value must be an object"))
        }
    }
}
