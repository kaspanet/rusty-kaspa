use crate::account as native;
use crate::imports::*;
use crate::tx::PaymentOutputs;
use crate::wasm::utxo::UtxoContext;
use kaspa_consensus_core::network::NetworkTypeT;
use kaspa_wallet_keys::keypair::Keypair;
use workflow_core::abortable::Abortable;

///
/// The `Account` class is a wallet account that can be used to send and receive payments.
///
///
///  @category Wallet API
#[wasm_bindgen(inspectable)]
#[derive(Clone, CastFromJs)]
pub struct Account {
    inner: Arc<dyn native::Account>,
    #[wasm_bindgen(getter_with_clone)]
    pub context: UtxoContext,
}

impl Account {
    pub async fn try_new(inner: Arc<dyn native::Account>) -> Result<Self> {
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
    pub fn balance_strings(&self, network_type: &NetworkTypeT) -> Result<JsValue> {
        match self.inner.balance() {
            Some(balance) => Ok(crate::wasm::Balance::from(balance).to_balance_strings(network_type)?.into()),
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

        todo!()
    }
}

impl From<Account> for Arc<dyn native::Account> {
    fn from(account: Account) -> Self {
        account.inner
    }
}

impl TryFrom<&JsValue> for Account {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> std::result::Result<Self, Self::Error> {
        Ok(Account::try_ref_from_js_value(js_value)?.clone())
    }
}

pub struct AccountSendArgs {
    pub outputs: PaymentOutputs,
    pub priority_fee_sompi: Option<u64>,
    pub include_fees_in_amount: bool,

    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub abortable: Abortable,
}

impl TryFrom<JsValue> for AccountSendArgs {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&js_value) {
            let outputs = object.get_cast::<PaymentOutputs>("outputs")?.into_owned();

            let priority_fee_sompi = object.get_u64("priorityFee").ok();
            let include_fees_in_amount = object.get_bool("includeFeesInAmount").unwrap_or(false);
            let abortable = object.get("abortable").ok().and_then(|v| Abortable::try_from(&v).ok()).unwrap_or_default();

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

pub struct AccountCreateArgs {}

impl TryFrom<JsValue> for AccountCreateArgs {
    type Error = Error;
    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let _keypair = object.try_get_cast::<Keypair>("keypair")?;
            let _public_key = object.try_get_cast::<Keypair>("keypair")?;

            Ok(AccountCreateArgs {})
        } else {
            Err(Error::custom("Account: supplied value must be an object"))
        }
    }
}
