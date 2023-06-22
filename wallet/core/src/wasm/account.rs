// use std::sync::atomic::AtomicBool;
// use std::sync::atomic::Ordering;

use crate::imports::*;
use crate::runtime;
use crate::runtime::AccountKind;
#[allow(unused_imports)]
use crate::secret::Secret;
#[allow(unused_imports)]
use crate::storage::PrvKeyDataId;
use js_sys::BigInt;
#[allow(unused_imports)]
use js_sys::Reflect;
#[allow(unused_imports)]
use workflow_core::abortable::Abortable;
// use wasm_bindgen::wasm_bindgen;
// use wasm_bindgen::prelude::*;
use crate::result::Result;
// use crate::iterator::*;

#[wasm_bindgen]
#[derive(Clone)]
pub struct Account {
    inner: Arc<runtime::Account>,
    receive_address_cache: Arc<Mutex<Option<Address>>>,
    change_address_cache: Arc<Mutex<Option<Address>>>,
    // abortable: Arc<AtomicBool>,
}

// #[wasm_bindgen(constructor)]
// pub fn constructor(_js_value: JsValue) -> std::result::Result<Account, JsError> {
//     todo!();
//     // Ok(js_value.try_into()?)
// }

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> JsValue {
        match self.inner.balance() {
            Some(balance) => BigInt::from(balance).into(),
            None => JsValue::UNDEFINED,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> AccountKind {
        self.inner.account_kind
    }

    #[wasm_bindgen(getter)]
    pub fn index(&self) -> u64 {
        self.inner.account_index
    }

    #[wasm_bindgen(getter, js_name = "privateKeyId")]
    pub fn private_key_id(&self) -> String {
        self.inner.prv_key_data_id.to_hex()
    }

    #[wasm_bindgen(getter, js_name = "isECDSA")]
    pub fn is_ecdsa(&self) -> bool {
        self.inner.ecdsa
    }

    #[wasm_bindgen(getter, js_name = "receiveAddress")]
    pub fn receive_address(&self) -> Address {
        self.receive_address_cache.lock().unwrap().clone().unwrap()
    }

    #[wasm_bindgen(getter, js_name = "changeAddress")]
    pub fn change_address(&self) -> Address {
        self.change_address_cache.lock().unwrap().clone().unwrap()
    }

    #[wasm_bindgen(js_name = "getReceiveAddress")]
    pub async fn get_receive_address(&self) -> Result<Address> {
        self.inner.derivation.receive_address_manager.current_address().await
    }

    #[wasm_bindgen(js_name = "createReceiveAddress")]
    pub async fn create_receive_address(&self) -> Result<Address> {
        self.inner.derivation.receive_address_manager.new_address().await
    }

    #[wasm_bindgen(js_name = "getChangeAddress")]
    pub async fn get_change_address(&self) -> Result<Address> {
        self.inner.derivation.change_address_manager.current_address().await
    }

    #[wasm_bindgen(js_name = "createChangeAddress")]
    pub async fn create_change_address(&self) -> Result<Address> {
        self.inner.derivation.change_address_manager.new_address().await
    }

    pub async fn scan(&self) -> Result<()> {
        self.inner.scan_utxos(None, None).await
    }

    pub async fn send(
        &self,
        // address: &Address,
        // amount_sompi: u64,
        // priority_fee_sompi: u64,
        // keydata: PrvKeyData,
        // payment_secret: Option<Secret>,
        // abortable: &Abortable,
        // ) -> Result<Vec<kaspa_hashes::Hash>> {
        js_value: JsValue,
    ) -> Result<JsValue> {
        let _args = SendArgs::try_from(js_value)?;

        todo!()
    }
}

impl Account {
    pub async fn update_addresses(&self) -> Result<()> {
        let receive_address = self.inner.derivation.receive_address_manager.current_address().await?;
        let change_address = self.inner.derivation.receive_address_manager.current_address().await?;
        self.receive_address_cache.lock().unwrap().replace(receive_address);
        self.change_address_cache.lock().unwrap().replace(change_address);
        Ok(())
    }
}

struct SendArgs {
    // outputs : Vec<(Address, u64)>,
    // priority_fee_sompi: u64,
    // wallet_secret: Option<Secret>,
    // payment_secret: Option<Secret>,
    // abortable: Abortable,
}

impl TryFrom<JsValue> for SendArgs {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if js_value.is_object() {
            let _object = Object::from(js_value);

            // let outputs = object.get_vec("outputs")?;

            // let outputs = {
            //     let outputs = Reflect::get(&object, &JsValue::from("outputs"))?;
            //     if outputs != JsValue::UNDEFINED {
            //         let array = outputs.dyn_into::<Array>().map_err(|err| Error::Custom(format!("`outputs` property must be an Array")))?;
            //         let vec = array.to_vec();

            //         // return Err(Error::MissingProperty(prop.to_string()));
            //     } else {
            //         let to = Reflect::get(&object, &JsValue::from("to"))?;

            //     }
            // };

            todo!()
        } else {
            Err("Argument to Account::send() must be an object".into())
        }
    }
}
