use crate::imports::*;
use crate::result::Result;
use crate::runtime;
use crate::secret::Secret;
use crate::storage::local::interface::LocalStore;
use crate::storage::PrvKeyDataId;
use crate::storage::{self, Hint};
use crate::wasm::wallet::account::Account;
use crate::wasm::wallet::keydata::PrvKeyDataInfo;
use kaspa_wrpc_client::wasm::RpcClient;
use kaspa_wrpc_client::WrpcEncoding;
use runtime::AccountKind;
use workflow_core::sendable::Sendable;
use workflow_wasm::channel::EventDispatcher;

#[wasm_bindgen(inspectable)]
#[derive(Clone)]
pub struct Wallet {
    pub(crate) wallet: Arc<runtime::Wallet>,
    #[wasm_bindgen(getter_with_clone)]
    pub rpc: RpcClient,
    #[wasm_bindgen(getter_with_clone)]
    pub events: EventDispatcher,
}

#[wasm_bindgen]
impl Wallet {
    #[wasm_bindgen(constructor)]
    pub fn constructor(js_value: JsValue) -> Result<Wallet> {
        let WalletCtorArgs { resident, network_id, encoding, url } = WalletCtorArgs::try_from(js_value)?;

        let store = Arc::new(LocalStore::try_new(resident)?);
        let rpc = RpcClient::new(
            url.unwrap_or("wrpc://127.0.0.1:17110".to_string()).as_str(),
            encoding.unwrap_or(WrpcEncoding::Borsh),
            None,
        )?;
        let rpc_api: Arc<DynRpcApi> = rpc.client().rpc_api().clone();
        let rpc_ctl = rpc.client().rpc_ctl().clone();
        let rpc_binding = Rpc::new(rpc_api, rpc_ctl);
        let wallet = Arc::new(runtime::Wallet::try_with_rpc(Some(rpc_binding), store, network_id)?);
        let events = EventDispatcher::default();

        Ok(Self { wallet, events, rpc })
    }

    pub async fn keys(&self) -> JsValue {
        let wallet = self.wallet.clone();
        let keys = self.wallet.keys().await.expect("Unable to access Wallet::keys iterator").then(move |item| {
            let wallet = wallet.clone();
            async move {
                match item {
                    Ok(prv_key_data_info) => Sendable::new(PrvKeyDataInfo::new(wallet, prv_key_data_info).into()),
                    Err(err) => Sendable::new(JsValue::from(err)),
                }
            }
        });

        AsyncStream::new(keys).into()
    }

    pub async fn accounts(&self) -> Result<JsValue> {
        self.account_iterator(JsValue::NULL).await
    }

    #[wasm_bindgen(js_name = "accountIterator")]
    pub async fn account_iterator(&self, prv_key_data_id_filter: JsValue) -> Result<JsValue> {
        let prv_key_data_id_filter = if prv_key_data_id_filter.is_falsy() {
            None
        } else {
            Some(PrvKeyDataId::from_hex(
                &prv_key_data_id_filter
                    .as_string()
                    .ok_or(Error::Custom("private key data id account filter must be a hex string or falsy".to_string()))?,
            )?)
        };

        let accounts = self
            .wallet
            .accounts(prv_key_data_id_filter)
            .await
            .unwrap_or_else(|err| panic!("Unable to access Wallet::account iterator: {err}"))
            .then(|item| async move {
                match item {
                    Ok(account) => Sendable::new(
                        Account::try_new(account).await.unwrap_or_else(|err| panic!("accountIterator (account): {err}")).into(),
                    ),
                    Err(err) => Sendable::new(JsValue::from(err)),
                }
            });

        Ok(AsyncStream::new(accounts).into())
    }

    #[wasm_bindgen(js_name = "isOpen")]
    pub fn is_open(&self) -> bool {
        self.wallet.is_open()
    }

    #[wasm_bindgen(js_name = "isSynced")]
    pub fn is_synced(&self) -> bool {
        self.wallet.is_synced()
    }

    #[wasm_bindgen(js_name = "descriptor")]
    pub fn descriptor(&self) -> Result<JsValue> {
        match self.wallet.descriptor()? {
            Some(desc) => Ok(JsValue::from(desc)),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    pub async fn exists(&self, name: JsValue) -> Result<bool> {
        let name =
            if name.is_falsy() {
                None
            } else {
                Some(name.as_string().ok_or(Error::Custom(
                    "Wallet::exists(): Wallet name must be a string (or falsy for default `kaspa`)".to_string(),
                ))?)
            };

        self.wallet.exists(name.as_deref()).await
    }

    #[wasm_bindgen(js_name = "createWallet")]
    pub async fn create_wallet(&self, wallet_args: &JsValue) -> Result<String> {
        let wallet_args: WalletCreateArgs = wallet_args.try_into()?;
        let descriptor = self.wallet.create_wallet(wallet_args.into()).await?;
        Ok(descriptor.unwrap_or_default())
    }

    #[wasm_bindgen(js_name = "createPrvKeyData")]
    pub async fn create_prv_key_data(&self, args: &JsValue) -> Result<Object> {
        let prv_key_data_args: PrvKeyDataCreateArgs = args.try_into()?;
        let (prv_key_data_id, mnemonic) = self.wallet.create_prv_key_data(prv_key_data_args.into()).await?;
        let object = Object::new();
        object.set("id", &JsValue::from(prv_key_data_id.to_hex()))?;
        object.set("mnemonic", &JsValue::from(mnemonic.phrase_string()))?;
        Ok(object)
    }

    #[wasm_bindgen(js_name = "createAccount")]
    pub async fn create_account(&self, prv_key_data_id: String, account_args: &JsValue) -> Result<JsValue> {
        let _account_args: AccountCreateArgs = account_args.try_into()?;
        let _prv_key_data_id =
            PrvKeyDataId::from_hex(&prv_key_data_id).map_err(|err| Error::KeyId(format!("{} : {err}", prv_key_data_id)))?;

        todo!()

        // match account_args.account_kind {
        //     AccountKind::Bip32 | AccountKind::Legacy => {
        //         let account = self.wallet.create_bip32_account(prv_key_data_id, account_args.into()).await?;
        //         Ok(Account::try_new(account).await?.into())
        //     }
        //     AccountKind::MultiSig => {
        //         todo!()
        //     }
        // }
    }

    pub async fn ping(&self) -> bool {
        self.wallet.ping().await
    }

    pub async fn start(&self) -> Result<()> {
        self.events.start_notification_task(self.wallet.multiplexer()).await?;
        self.wallet.start().await?;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.wallet.stop().await?;
        self.events.stop_notification_task().await?;
        Ok(())
    }

    pub async fn connect(&self, args: JsValue) -> Result<()> {
        self.rpc.connect(args).await?;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.rpc.client.disconnect().await?;
        Ok(())
    }
}

#[derive(Default)]
struct WalletCtorArgs {
    resident: bool,
    network_id: Option<NetworkId>,
    encoding: Option<WrpcEncoding>,
    url: Option<String>,
}

impl TryFrom<JsValue> for WalletCtorArgs {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self> {
        if let Some(object) = Object::try_from(&js_value) {
            let resident = object.get_value("resident")?.as_bool().unwrap_or(false);

            let network_id = object.try_get::<NetworkId>("networkId")?;
            let encoding = object.try_get::<WrpcEncoding>("encoding")?;

            let url = object.get_value("url")?.as_string();

            Ok(Self { resident, network_id, encoding, url })
        } else {
            Ok(WalletCtorArgs::default())
        }
    }
}

struct WalletCreateArgs {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub user_hint: Option<Hint>,
    pub wallet_secret: Secret,
    pub overwrite_wallet_storage: bool,
}

impl TryFrom<&JsValue> for WalletCreateArgs {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(js_value) {
            Ok(WalletCreateArgs {
                title: object.try_get_string("title")?,
                filename: object.try_get_string("filename")?,
                user_hint: object.try_get_string("hint")?.map(Hint::from),
                wallet_secret: object.get_string("walletSecret")?.into(),
                overwrite_wallet_storage: object.try_get_bool("overwrite")?.unwrap_or(false),
            })
        } else if let Some(secret) = js_value.as_string() {
            Ok(WalletCreateArgs {
                title: None,
                filename: None,
                user_hint: None,
                wallet_secret: secret.into(),
                overwrite_wallet_storage: false,
            })
        } else {
            Err("WalletCreateArgs argument must be an object or a secret".into())
        }
    }
}

impl From<WalletCreateArgs> for runtime::WalletCreateArgs {
    fn from(args: WalletCreateArgs) -> Self {
        Self {
            title: args.title,
            filename: args.filename,
            user_hint: args.user_hint,
            wallet_secret: args.wallet_secret,
            overwrite_wallet_storage: args.overwrite_wallet_storage,
        }
    }
}

struct PrvKeyDataCreateArgs {
    pub name: Option<String>,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub mnemonic: Option<String>,
}

impl TryFrom<&JsValue> for PrvKeyDataCreateArgs {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(js_value) {
            Ok(PrvKeyDataCreateArgs {
                name: object.try_get_string("name")?,
                wallet_secret: object.get_string("walletSecret")?.into(),
                payment_secret: object.try_get_string("paymentSecret")?.map(|s| s.into()),
                mnemonic: object.try_get_string("mnemonic")?,
            })
        } else if let Some(secret) = js_value.as_string() {
            Ok(PrvKeyDataCreateArgs { name: None, wallet_secret: secret.into(), payment_secret: None, mnemonic: None })
        } else {
            Err("PrvKeyDataCreateArgs argument must be an object or a secret".into())
        }
    }
}

impl From<PrvKeyDataCreateArgs> for runtime::PrvKeyDataCreateArgs {
    fn from(args: PrvKeyDataCreateArgs) -> Self {
        Self { name: args.name, wallet_secret: args.wallet_secret, payment_secret: args.payment_secret, mnemonic: args.mnemonic }
    }
}

// impl Drop for PrvKeyDataCreateArgs {
//     fn drop(&mut self) {
//         self.wallet_secret.clear();
//         self.payment_secret.clear();
//         self.mnemonic.zeroize();
//     }
// }

struct AccountCreateArgs {
    pub name: Option<String>,
    pub title: Option<String>,
    pub account_kind: storage::AccountKind,
    pub wallet_secret: Secret,
    pub payment_secret: Option<String>,
}

impl TryFrom<&JsValue> for AccountCreateArgs {
    type Error = Error;
    fn try_from(js_value: &JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(js_value) {
            let account_kind = object.get::<AccountKind>("accountKind")?;
            // let account_kind = if let Some(kind) = kind.as_f64() {
            //     AccountKind::try_from(kind as u8)?
            // } else if let Some(kind) = kind.as_string() {
            //     AccountKind::from_str(kind.as_str())?
            // } else if kind.is_undefined() {
            //     AccountKind::default()
            // } else {
            //     return Err(Error::Custom("AccountCreateArgs is missing `accountKind` property".to_string()));
            // };

            Ok(AccountCreateArgs {
                name: object.try_get_string("name")?,
                title: object.try_get_string("title")?,
                account_kind,
                wallet_secret: object.get_string("walletSecret")?.into(),
                payment_secret: object.try_get_string("paymentSecret")?,
            })
        } else {
            Err("AccountCreateArgs argument must be an object".into())
        }
    }
}

impl From<AccountCreateArgs> for runtime::AccountCreateArgs {
    fn from(args: AccountCreateArgs) -> Self {
        runtime::AccountCreateArgs {
            name: args.name,
            title: args.title,
            account_kind: args.account_kind,
            wallet_secret: args.wallet_secret,
            payment_secret: args.payment_secret.map(|s| s.into()),
        }
    }
}
