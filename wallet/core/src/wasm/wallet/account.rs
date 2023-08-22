use crate::imports::*;
use crate::result::Result;
use crate::runtime;
use crate::secret::Secret;
use crate::tx::PaymentOutputs;
use crate::wasm::utxo::UtxoContext;
use kaspa_consensus_wasm::Keypair;
use workflow_core::abortable::Abortable;
use workflow_wasm::abi::ref_from_abi;

#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct Account {
    inner: Arc<dyn runtime::Account>,
    #[wasm_bindgen(getter_with_clone)]
    pub context: UtxoContext,
}

impl Account {
    pub async fn try_new(inner: Arc<dyn runtime::Account>) -> Result<Self> {
        let context = inner.utxo_context().clone();
        Ok(Self { inner, context: context.into() })
    }
}

#[wasm_bindgen]
impl Account {
    pub fn ctor(js_value: JsValue) -> Result<Account> {
        let AccountCreateArgs {} = js_value.try_into()?;

        todo!();

        // Ok(account)
    }

    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> JsValue {
        match self.inner.balance() {
            Some(balance) => crate::wasm::Balance::from(balance).into(),
            None => JsValue::UNDEFINED,
        }
    }

    #[wasm_bindgen(getter, js_name = "type")]
    pub fn account_kind(&self) -> String {
        self.inner.account_kind().to_string()
    }

    #[wasm_bindgen(js_name = balanceStrings)]
    pub fn balance_strings(&self, network_type: JsValue) -> Result<JsValue> {
        match self.inner.balance() {
            Some(balance) => Ok(crate::wasm::Balance::from(balance).as_strings(network_type)?.into()),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    #[wasm_bindgen(getter, js_name = "receiveAddress")]
    pub fn receive_address(&self) -> Result<String> {
        Ok(self.inner.receive_address()?.to_string())
    }

    #[wasm_bindgen(getter, js_name = "changeAddress")]
    pub fn change_address(&self) -> Result<String> {
        Ok(self.inner.change_address()?.to_string())
    }

    #[wasm_bindgen(js_name = "deriveReceiveAddress")]
    pub async fn derive_receive_address(&self) -> Result<Address> {
        let account = self.inner.clone().as_derivation_capable()?;
        let receive_address = account.new_receive_address().await?;
        Ok(receive_address)
    }

    #[wasm_bindgen(js_name = "deriveChangeAddress")]
    pub async fn derive_change_address(&self) -> Result<Address> {
        let account = self.inner.clone().as_derivation_capable()?;
        let change_address = account.new_change_address().await?;
        Ok(change_address)
    }

    pub async fn scan(&self) -> Result<()> {
        self.inner.clone().scan(None, None).await
    }

    pub async fn send(&self, js_value: JsValue) -> Result<JsValue> {
        let _args = AccountSendArgs::try_from(js_value)?;

        // self.inner.clone().send(

        // self: Arc<Self>,
        // destination: PaymentDestination,
        // priority_fee_sompi: Fees,
        // payload: Option<Vec<u8>>,
        // wallet_secret: Secret,
        // payment_secret: Option<Secret>,
        // abortable: &Abortable,
        // notifier: Option<GenerationNotifier>,

        // ).await;

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

impl From<Account> for Arc<dyn runtime::Account> {
    fn from(account: Account) -> Self {
        account.inner
    }
}

impl TryFrom<JsValue> for Account {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        Ok(ref_from_abi!(Account, &js_value)?)
    }
}

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

// self: Arc<Self>,
// destination: PaymentDestination,
// priority_fee_sompi: Fees,
// payload: Option<Vec<u8>>,
// wallet_secret: Secret,
// payment_secret: Option<Secret>,
// abortable: &Abortable,
// notifier: Option<GenerationNotifier>,

pub struct AccountSendArgs {
    // pub destination : PaymentDestination,
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
            let outputs = object.get::<PaymentOutputs>("outputs")?;

            let priority_fee_sompi = object.get_u64("priorityFee").ok();
            let include_fees_in_amount = object.get_bool("includeFeesInAmount").unwrap_or(false);
            let abortable = object.get("abortable").ok().and_then(|v| ref_from_abi!(Abortable, &v).ok()).unwrap_or_default();

            let wallet_secret = object.get_string("walletSecret")?.into();
            let payment_secret = object.get_value("paymentSecret")?.as_string().map(|s| s.into());

            let send_args =
                AccountSendArgs { outputs, priority_fee_sompi, include_fees_in_amount, wallet_secret, payment_secret, abortable };

            Ok(send_args)
        } else {
            Err("Argument to Account::send() must be an object".into())
        }
    }
}

pub struct AccountCreateArgs {
    // rpc: RpcClient,
    // network_id: NetworkId,
}

impl TryFrom<JsValue> for AccountCreateArgs {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let _keypair = object.try_get::<Keypair>("keypair")?;
            let _public_key = object.try_get::<Keypair>("keypair")?;

            Ok(AccountCreateArgs {})
        } else {
            Err(Error::custom("Account: suppliedd value must be an object"))
        }
    }
}
