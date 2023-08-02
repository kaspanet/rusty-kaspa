use crate::imports::*;
use crate::result::Result;
use crate::runtime;
use crate::secret::Secret;
use crate::tx::PaymentOutputs;
use crate::wasm;
use workflow_core::abortable::Abortable;
use workflow_wasm::abi::ref_from_abi;

pub struct CacheInner {
    receive_address: Address,
    change_address: Address,
}

#[derive(Clone)]
pub struct Cache {
    inner: Arc<Mutex<CacheInner>>,
}

impl Cache {
    pub async fn try_new(account: &Arc<runtime::Account>) -> Result<Self> {
        let inner = Self::make_inner(account).await?;
        Ok(Cache { inner: Arc::new(Mutex::new(inner)) })
    }

    pub async fn update(&self, account: &Arc<runtime::Account>) -> Result<()> {
        *self.inner.lock().unwrap() = Self::make_inner(account).await?;
        Ok(())
    }

    pub async fn make_inner(account: &Arc<runtime::Account>) -> Result<CacheInner> {
        let receive_address = account.derivation.receive_address_manager.current_address().await?;
        let change_address = account.derivation.change_address_manager.current_address().await?;
        Ok(CacheInner { receive_address, change_address })
    }

    pub fn receive_address(&self) -> Address {
        self.inner.lock().unwrap().receive_address.clone()
    }

    pub fn change_address(&self) -> Address {
        self.inner.lock().unwrap().change_address.clone()
    }
}

#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct Account {
    inner: Arc<runtime::Account>,
    cache: Cache,
}

impl Account {
    pub async fn try_new(inner: Arc<runtime::Account>) -> Result<Self> {
        let cache = Cache::try_new(&inner).await?;

        Ok(Self { inner, cache })
    }
}

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> JsValue {
        match self.inner.balance() {
            Some(balance) => wasm::Balance::from(balance).into(),
            None => JsValue::UNDEFINED,
        }
    }

    #[wasm_bindgen(getter, js_name = "accountKind")]
    pub fn account_kind(&self) -> String {
        self.inner.account_kind.to_string()
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
    pub fn receive_address(&self) -> String {
        self.cache.receive_address().to_string()
    }

    #[wasm_bindgen(getter, js_name = "changeAddress")]
    pub fn change_address(&self) -> String {
        self.cache.change_address().to_string()
    }

    #[wasm_bindgen(js_name = "getReceiveAddress")]
    pub async fn get_receive_address(&self) -> Result<Address> {
        self.inner.derivation.receive_address_manager.current_address().await
    }

    #[wasm_bindgen(js_name = "createReceiveAddress")]
    pub async fn create_receive_address(&self) -> Result<Address> {
        let receive_address = self.inner.derivation.receive_address_manager.new_address().await?;
        self.cache.inner.lock().unwrap().receive_address = receive_address.clone();
        Ok(receive_address)
    }

    #[wasm_bindgen(js_name = "getChangeAddress")]
    pub async fn get_change_address(&self) -> Result<Address> {
        self.inner.derivation.change_address_manager.current_address().await
    }

    #[wasm_bindgen(js_name = "createChangeAddress")]
    pub async fn create_change_address(&self) -> Result<Address> {
        let change_address = self.inner.derivation.change_address_manager.new_address().await?;
        self.cache.inner.lock().unwrap().change_address = change_address.clone();
        Ok(change_address)
    }

    pub async fn scan(&self) -> Result<()> {
        self.inner.scan(None, None).await
    }

    pub async fn send(&self, js_value: JsValue) -> Result<JsValue> {
        let _args = AccountSendArgs::try_from(js_value)?;

        // self.inner
        //     .send_v1(
        //         &args.outputs,
        //         args.priority_fee_sompi,
        //         args.include_fees_in_amount,
        //         args.wallet_secret,
        //         args.payment_secret,
        //         &args.abortable,
        //     )
        //     .await?;

        todo!()
    }
}

impl Account {
    pub async fn update(&self) -> Result<()> {
        self.cache.update(&self.inner).await
    }
}
//     pub async fn update_addresses(&self) -> Result<()> {
//         let receive_address = self.inner.derivation.receive_address_manager.current_address().await?;
//         let change_address = self.inner.derivation.receive_address_manager.current_address().await?;
//         self.receive_address_cache.lock().unwrap().replace(receive_address);
//         self.change_address_cache.lock().unwrap().replace(change_address);
//         Ok(())
//     }
// }

// impl From<Arc<runtime::Account>> for Account {
//     fn from(inner: Arc<runtime::Account>) -> Self {
//         Account { inner, cache : Cache::default() }
//     }
// }

// pub enum IterResult<T, E> {
//     Ok(T),
//     Err(E),
// }

// impl<T,E> From<Result<T,E>> for IterResult<T,E> {
//     fn from(result: Result<T,E>) -> IterResult<T,E> {
//         match result {
//             Ok(t) => IterResult::Ok(t),
//             Err(e) => IterResult::Err(e),
//         }
//     }
// }

// impl From<IterResult<Arc<runtime::Account>>> for JsValue {
//     fn from(account: Result<Arc<runtime::Account>>) -> Self {
//         account.map(|account| account.into())
//     }
// }

pub struct AccountSendArgs {
    pub outputs: PaymentOutputs,
    pub priority_fee_sompi: Option<u64>,
    pub include_fees_in_amount: bool,

    // pub utxos: Option<Vec<Arc<UtxoEntryReference>>>,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub abortable: Abortable,
}

impl TryFrom<JsValue> for AccountSendArgs {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&js_value) {
            let outputs = PaymentOutputs::try_from(object.get("outputs")?)?;

            let priority_fee_sompi = object.get_u64("priorityFee").ok();
            let include_fees_in_amount = object.get_bool("includeFeesInAmount").unwrap_or(false);
            let abortable = object.get("abortable").ok().and_then(|v| ref_from_abi!(Abortable, &v).ok()).unwrap_or_default();

            let wallet_secret = object.get_string("walletSecret")?.into();
            let payment_secret = object.get("paymentSecret")?.as_string().map(|s| s.into());

            let send_args =
                AccountSendArgs { outputs, priority_fee_sompi, include_fees_in_amount, wallet_secret, payment_secret, abortable };

            Ok(send_args)
        } else {
            Err("Argument to Account::send() must be an object".into())
        }
    }
}
