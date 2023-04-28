use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{self, KeyDataId, PrvKeyData};
#[allow(unused_imports)]
use crate::{account::Account, accounts::gen0, accounts::gen0::import::*, accounts::gen1, accounts::gen1::import::*};
use futures::future::join_all;
use futures::{select, FutureExt};
use kaspa_addresses::Prefix as AddressPrefix;
use kaspa_bip32::Mnemonic;
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{
    api::rpc::RpcApi,
    notify::{connection::ChannelConnection, mode::NotificationMode},
    Notification,
};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use std::path::PathBuf;
use std::sync::Arc;
use storage::{Payload, PubKeyData, Store, WalletSettings};
use workflow_core::channel::{Channel, DuplexChannel, Multiplexer, Receiver};
use workflow_core::task::spawn;
use workflow_log::log_error;
use workflow_rpc::client::Ctl;

#[derive(Clone)]
pub struct AccountCreateArgs {
    pub title: String,
    pub account_kind: storage::AccountKind,
    pub wallet_password: Secret,
    pub payment_password: Option<Secret>,
    pub override_wallet: bool,
}

impl AccountCreateArgs {
    pub fn new(title: String, account_kind: storage::AccountKind, wallet_password: Secret, payment_password: Option<Secret>) -> Self {
        Self { title, account_kind, wallet_password, payment_password, override_wallet: false }
    }
}

#[derive(Clone)]
pub enum Events {
    Connect,
    Disconnect,
}

//#[derive(Clone)]
pub struct Inner {
    // accounts: Vec<Arc<dyn WalletAccountTrait>>,
    accounts: Mutex<Vec<Arc<Account>>>,
    listener_id: Mutex<ListenerId>,
    notification_receiver: Receiver<Notification>,
    #[allow(dead_code)] //TODO: remove me
    ctl_receiver: Receiver<Ctl>,
    pub task_ctl: DuplexChannel,
    multiplexer: Multiplexer<Events>,
    pub selected_account: Mutex<Option<Arc<Account>>>,
}

/// `Wallet` data structure
#[derive(Clone)]
#[wasm_bindgen]
pub struct Wallet {
    #[wasm_bindgen(skip)]
    pub rpc: Arc<KaspaRpcClient>,
    inner: Arc<Inner>,
}

impl Wallet {
    pub async fn try_new() -> Result<Wallet> {
        Wallet::try_with_rpc(None).await
    }

    pub async fn try_with_rpc(rpc: Option<Arc<KaspaRpcClient>>) -> Result<Wallet> {
        let _master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let rpc = if let Some(rpc) = rpc {
            rpc
        } else {
            // Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::Direct, "wrpc://localhost:17110")?)
            Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::MultiListeners, "wrpc://127.0.0.1:17110")?)
        };

        let (listener_id, notification_receiver) = match rpc.notification_mode() {
            NotificationMode::MultiListeners => {
                let notification_channel = Channel::unbounded();
                let connection = ChannelConnection::new(notification_channel.sender);
                (rpc.register_new_listener(connection), notification_channel.receiver)
            }
            NotificationMode::Direct => (ListenerId::default(), rpc.notification_channel_receiver()),
        };

        let ctl_receiver = rpc.ctl_channel_receiver();

        let multiplexer = Multiplexer::new();

        let wallet = Wallet {
            rpc,
            inner: Arc::new(Inner {
                accounts: Mutex::new(vec![]), //vec![Arc::new(WalletAccount::from_master_xprv(master_xprv, false, 0).await?)],
                notification_receiver,
                listener_id: Mutex::new(listener_id),
                ctl_receiver,
                multiplexer,
                task_ctl: DuplexChannel::oneshot(),
                selected_account: Mutex::new(None),
            }),
        };

        Ok(wallet)
    }

    pub async fn clear(&self) -> Result<()> {
        self.inner.accounts.lock()?.clear();
        Ok(())
    }

    // pub fn load_accounts(&self, stored_accounts: Vec<storage::Account>) => Result<()> {
    pub async fn load_accounts(&self, secret: Secret, prefix: AddressPrefix) -> Result<()> {
        let store = storage::Store::new(crate::storage::DEFAULT_WALLET_FILE)?;
        let wallet = storage::Wallet::try_load(&store).await?;
        let payload = wallet.payload.decrypt::<storage::Payload>(secret)?;

        let accounts =
            payload.as_ref().accounts.iter().map(|stored| Account::try_new_from_storage(self, stored, prefix)).collect::<Vec<_>>();
        let accounts = join_all(accounts).await.into_iter().collect::<Result<Vec<_>>>()?;
        let accounts = accounts.into_iter().map(Arc::new).collect::<Vec<_>>();

        *self.inner.accounts.lock()? = accounts;

        Ok(())
    }

    pub async fn get_account_keydata(&self, id: Option<KeyDataId>, secret: Secret) -> Result<Option<PrvKeyData>> {
        let id = if let Some(id) = id { id } else { return Ok(None) };

        let store = storage::Store::new(crate::storage::DEFAULT_WALLET_FILE)?;
        let wallet = storage::Wallet::try_load(&store).await?;
        let payload = wallet.payload.decrypt::<storage::Payload>(secret)?;
        let key = payload.as_ref().prv_key_data.iter().find(|k| k.id == id);

        Ok(key.cloned())
    }

    pub async fn is_account_key_encrypted(&self, account: &Account, secret: Secret) -> Result<bool> {
        let id = if let Some(id) = account.prv_key_data_id { id } else { return Ok(false) };

        let store = storage::Store::new(crate::storage::DEFAULT_WALLET_FILE)?;
        let wallet = storage::Wallet::try_load(&store).await?;
        let payload = wallet.payload.decrypt::<storage::Payload>(secret)?;
        let key = payload.as_ref().prv_key_data.iter().find(|k| k.id == id);

        if let Some(key) = key {
            Ok(key.payload.is_encrypted())
        } else {
            Ok(false)
        }
    }

    pub fn rpc(&self) -> &Arc<KaspaRpcClient> {
        &self.rpc
    }

    pub fn multiplexer(&self) -> &Multiplexer<Events> {
        &self.inner.multiplexer
    }

    // intended for starting async management tasks
    pub async fn start(&self) -> Result<()> {
        // internal event loop
        self.start_task().await?;
        // rpc services (notifier)
        self.rpc.start().await?;
        // start async RPC connection

        // TODO handle reconnect flag
        // self.rpc.connect_as_task()?;
        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(&self) -> Result<()> {
        self.rpc.stop().await?;
        self.stop_task().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        *self.inner.listener_id.lock().unwrap()
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.inner.notification_receiver.clone()
    }

    // ~~~

    pub async fn get_info(&self) -> Result<String> {
        let v = self.rpc.get_info().await?;
        Ok(format!("{v:#?}").replace('\n', "\r\n"))
    }

    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.rpc.start_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.rpc.stop_notify(self.listener_id(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {})).await?;
        Ok(())
    }

    pub async fn ping(&self) -> Result<()> {
        Ok(self.rpc.ping().await?)
    }

    pub async fn broadcast(&self) -> Result<()> {
        Ok(())
    }

    pub async fn create(&self, args: &AccountCreateArgs) -> Result<PathBuf> {
        let store = Store::new(storage::DEFAULT_WALLET_FILE)?;
        if !args.override_wallet && store.exists().await? {
            return Err(Error::WalletAlreadyExists);
        }

        let prefix = AddressPrefix::Mainnet;

        let xpub_prefix = kaspa_bip32::Prefix::XPUB;

        let payment_password = args.payment_password.clone().unwrap_or(args.wallet_password.clone());

        let mnemonic = Mnemonic::create_random()?;
        let mut payload = Payload::default();
        let account_index = 0;
        let prv_key_data = payload.add_keydata(mnemonic.phrase_string(), payment_password.clone())?;
        let xpub_key = prv_key_data.create_xpub(payment_password, args.account_kind, account_index).await?;
        let pub_key_data = PubKeyData::new(vec![xpub_key.to_string(Some(xpub_prefix))], None, None);

        let stored_account = storage::Account::new(
            args.title.replace(' ', "-").to_lowercase(),
            args.title.clone(),
            args.account_kind,
            account_index,
            false,
            pub_key_data,
            Some(prv_key_data.id),
            false,
            1,
            0,
        );

        let account_id = stored_account.id.clone();
        payload.accounts.push(stored_account.clone());

        let settings = WalletSettings::new(account_id);
        storage::Wallet::try_store(&store, args.wallet_password.clone(), settings, payload).await?;

        // -
        self.clear().await?;

        let account = Arc::new(Account::try_new_from_storage(self, &stored_account, prefix).await?);
        self.inner.accounts.lock().unwrap().push(account.clone());

        self.select(Some(account)).await?;

        Ok(store.filename().clone())
    }

    pub async fn dump_unencrypted(&self) -> Result<()> {
        Ok(())
    }

    pub async fn select(&self, account: Option<Arc<Account>>) -> Result<()> {
        // log_info!(target: "term","selecting account");
        log_info!("selecting account");
        *self.inner.selected_account.lock().unwrap() = account.clone();
        if let Some(account) = account {
            account.start().await?;
        }
        Ok(())
    }

    pub fn account(&self) -> Result<Arc<Account>> {
        self.inner.selected_account.lock().unwrap().clone().ok_or_else(|| Error::AccountSelection)
    }

    pub fn accounts(&self) -> Vec<Arc<Account>> {
        self.inner.accounts.lock().unwrap().clone()
    }

    // pub async fn import_gen0_keydata(self: &Arc<Wallet>, import_secret: Secret, wallet_secret: Secret) -> Result<()> {
    pub async fn import_gen0_keydata(&self, import_secret: Secret, wallet_secret: Secret) -> Result<()> {
        let keydata = load_v0_keydata(&import_secret).await?;

        let store = storage::Store::new(storage::DEFAULT_WALLET_FILE)?;
        let wallet = storage::Wallet::try_load(&store).await?;
        let mut payload = wallet.payload.decrypt::<Payload>(wallet_secret).unwrap();
        let payload = payload.as_mut();

        let prv_key_data = PrvKeyData::new_from_mnemonic(&keydata.mnemonic);

        // TODO: integrate address generation
        // let derivation_path = gen1::WalletAccount::build_derivate_path(false, 0, Some(kaspa_bip32::AddressType::Receive))?;
        // let xkey = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?.derive_path(derivation_path)?;

        let stored_account = storage::Account::new(
            "imported-wallet".to_string(),       // name
            "Imported Wallet".to_string(),       // title
            storage::AccountKind::V0,            // kind
            0,                                   // account index
            false,                               // public visibility
            PubKeyData::new(vec![], None, None), // TODO - pub keydata
            Some(prv_key_data.id),               // privkey id
            false,                               // ecdsa
            1,                                   // min signatures
            0,                                   // cosigner_index
        );

        let prefix = AddressPrefix::Mainnet;

        let runtime_account = Account::try_new_from_storage(self, &stored_account, prefix).await?;

        payload.prv_key_data.push(prv_key_data);
        // TODO - prevent multiple addition of the same private key
        payload.accounts.push(stored_account);

        self.inner.accounts.lock().unwrap().push(Arc::new(runtime_account));

        Ok(())
    }

    pub async fn import_gen1_keydata(self: &Arc<Wallet>, secret: Secret) -> Result<()> {
        let _keydata = load_v1_keydata(&secret).await?;

        Ok(())
    }

    pub async fn start_task(&self) -> Result<()> {
        let _self = self.clone();
        let ctl_receiver = self.rpc.ctl_channel_receiver();
        // let task_ctl = self.inner.lock().unwrap().task_ctl.clone();
        let multiplexer = self.inner.multiplexer.clone();
        let task_ctl_receiver = self.inner.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.inner.task_ctl.response.sender.clone();
        // let multiplexer = multiplexer.clone();

        spawn(async move {
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    msg = ctl_receiver.recv().fuse() => {
                        if let Ok(msg) = msg {
                            match msg {
                                Ctl::Open => {
                                    //println!("Ctl::Open:::::::");
                                    multiplexer.broadcast(Events::Connect).await.unwrap_or_else(|err| log_error!("{err}"));
                                    // self_.connect().await?;
                                },
                                Ctl::Close => {
                                    multiplexer.broadcast(Events::Disconnect).await.unwrap_or_else(|err| log_error!("{err}"));
                                    // self_.disconnect().await?;
                                }
                            }
                        }
                    }
                }
            }

            task_ctl_sender.send(()).await.unwrap();
        });
        Ok(())
    }

    pub async fn stop_task(&self) -> Result<()> {
        self.inner.task_ctl.signal(()).await.expect("Wallet::stop_task() `signal` error");
        Ok(())
    }

    // pub async fn connect(&self) -> Result<()> {
    //     for account in self.inner.accounts.iter() {
    //         account.connect().await?;
    //     }
    //     Ok(())
    // }

    // pub async fn disconnect(&self) -> Result<()> {
    //     for account in self.inner.accounts.iter() {
    //         account.disconnect().await?;
    //     }
    //     Ok(())
    // }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod test {
    use std::{str::FromStr, thread::sleep, time};

    use super::*;
    use crate::{
        signer::sign_mutable_transaction,
        // Signer,
        tx::MutableTransaction,
        utxo::{
            //SelectionContext,
            UtxoOrdering,
            UtxoSet,
        },
    };
    //use kaspa_bip32::{ExtendedPrivateKey, SecretKey};

    // TODO - re-export subnets
    use crate::tx::Transaction;
    use crate::tx::TransactionInput;
    use crate::tx::TransactionOutput;
    use kaspa_consensus_core::subnets::SubnetworkId;
    //use kaspa_consensus_core::tx::ScriptPublicKey;
    //use kaspa_consensus_core::tx::MutableTransaction;
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, SecretKey};
    use kaspa_txscript::pay_to_address_script;

    async fn get_utxos_set_by_addresses(rpc: Arc<KaspaRpcClient>, addresses: Vec<Address>) -> Result<UtxoSet> {
        let utxos = rpc.get_utxos_by_addresses(addresses).await?;
        let utxo_set = UtxoSet::new();
        for utxo in utxos {
            utxo_set.insert(utxo.into());
        }
        Ok(utxo_set)
    }

    #[allow(dead_code)]
    // #[tokio::test]
    async fn wallet_test() -> Result<()> {
        println!("Creating wallet...");
        let wallet = Arc::new(Wallet::try_new().await?);
        // let stored_accounts = vec![StoredWalletAccount{
        //     private_key_index: 0,
        //     account_kind: crate::storage::AccountKind::Bip32,
        //     name: "Default Account".to_string(),
        //     title: "Default Account".to_string(),
        // }];

        // wallet.load_accounts(stored_accounts);

        let rpc = wallet.rpc();

        let _connect_result = rpc.connect(true).await;
        //println!("connect_result: {_connect_result:?}");

        let _result = wallet.start().await;
        //println!("wallet.task(): {_result:?}");
        let result = wallet.get_info().await;
        println!("wallet.get_info(): {result:#?}");

        let address = Address::try_from("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd")?;

        let utxo_set = self::get_utxos_set_by_addresses(rpc.clone(), vec![address.clone()]).await?;

        let utxo_set_balance = utxo_set.calculate_balance().await?;
        println!("get_utxos_by_addresses: {utxo_set_balance:?}");

        let utxo_selection = utxo_set.select(100000, UtxoOrdering::AscendingAmount).await?;

        //let payload = vec![];
        let to_address = Address::try_from("kaspatest:qpakxqlesqywgkq7rg4wyhjd93kmw7trkl3gpa3vd5flyt59a43yyn8vu0w8c")?;
        //let outputs = Outputs { outputs: vec![Output::new(to_address, 100000, None)] };
        //let vtx = VirtualTransaction::new(utxo_selection, &outputs, payload);

        //vtx.sign();
        let utxo = (*utxo_selection.selected_entries[0].utxo).clone();
        //utxo.utxo_entry.is_coinbase = false;
        let selected_entries = vec![utxo];

        let entries = &selected_entries;

        let inputs = selected_entries
            .iter()
            .enumerate()
            .map(|(sequence, utxo)| TransactionInput::new(utxo.outpoint.clone(), vec![], sequence as u64, 0))
            .collect::<Vec<TransactionInput>>();

        let tx = Transaction::new(
            0,
            inputs,
            vec![
                TransactionOutput::new(1000, &pay_to_address_script(&to_address)),
                // TransactionOutput::new() { value: 1000, script_public_key: pay_to_address_script(&to_address) },
                //TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        )?;

        let mtx = MutableTransaction::new(&tx, &(*entries).clone().into());

        let derivation_path =
            gen1::WalletDerivationManager::build_derivate_path(false, 0, None, Some(kaspa_bip32::AddressType::Receive))?;

        let xprv = "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";
        //let (xkey, _attrs) = WalletAccount::create_extended_key_from_xprv(xprv, false, 0).await?;

        let xkey = ExtendedPrivateKey::<SecretKey>::from_str(xprv)?.derive_path(derivation_path)?;

        let xkey = xkey.derive_child(ChildNumber::new(0, false)?)?;

        // address test
        let address_test = Address::new(Prefix::Testnet, Version::PubKey, &xkey.public_key().to_bytes()[1..]);
        let address_str: String = address_test.clone().into();
        assert_eq!(address, address_test, "Address dont match");
        println!("address: {address_str}");

        let private_keys = vec![
            //xkey.private_key().into()
            xkey.to_bytes(),
        ];

        println!("mtx: {mtx:?}");

        //let signer = Signer::new(private_keys)?;
        let mtx = sign_mutable_transaction(mtx, &private_keys, true)?;
        //println!("mtx: {mtx:?}");

        let utxo_set = self::get_utxos_set_by_addresses(rpc.clone(), vec![to_address.clone()]).await?;
        let to_balance = utxo_set.calculate_balance().await?;
        println!("to address balance before tx submit: {to_balance:?}");

        let result = rpc.submit_transaction(mtx.try_into()?, false).await?;

        println!("tx submit result, {:?}", result);
        println!("sleep for 5s...");
        sleep(time::Duration::from_millis(5000));
        let utxo_set = self::get_utxos_set_by_addresses(rpc.clone(), vec![to_address.clone()]).await?;
        let to_balance = utxo_set.calculate_balance().await?;
        println!("to address balance after tx submit: {to_balance:?}");

        Ok(())
    }
}
