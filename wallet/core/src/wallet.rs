use crate::account::{AccountKind, AccountConfig};
use crate::error::Error;
use crate::{accounts::*, account::Account};
use crate::result::Result;
use kaspa_notify::{
    listener::ListenerId,
    scope::{Scope, VirtualDaaScoreChangedScope},
};
use kaspa_rpc_core::{api::rpc::RpcApi, notify::connection::ChannelConnection, Notification};
use kaspa_wrpc_client::{KaspaRpcClient, NotificationMode, WrpcEncoding};
use workflow_core::channel::DuplexChannel;
use workflow_log::log_error;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
#[allow(unused_imports)]
use workflow_core::channel::{Channel, Receiver, Multiplexer};
use workflow_rpc::client::Ctl;
use crate::storage::StoredWalletAccount;
use workflow_core::task::spawn;
use futures::{select, FutureExt};


#[derive(Clone)]
pub enum Events {
    Connect,
    Disconnect,
}

//#[derive(Clone)]
pub struct Inner {
    // accounts: Vec<Arc<dyn WalletAccountTrait>>,
    accounts : Mutex<Vec<Arc<Account>>>,
    listener_id: Mutex<ListenerId>,
    notification_receiver: Receiver<Notification>,
    ctl_receiver : Receiver<Ctl>,
    pub task_ctl : DuplexChannel,
    multiplexer : Multiplexer<Events>,
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
        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let rpc = if let Some(rpc) = rpc {
            rpc
        } else {
            // Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::Direct, "wrpc://localhost:17110")?)
            Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::MultiListeners, "wrpc://localhost:17110")?)
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
                listener_id : Mutex::new(listener_id),
                ctl_receiver,
                multiplexer,
                task_ctl: DuplexChannel::oneshot(),
                selected_account: Mutex::new(None),
            }),
        };

        Ok(wallet)
    }

    pub fn load_accounts(&self, stored_accounts : Vec<StoredWalletAccount>) {
        let accounts = stored_accounts.iter().map(|stored| {
            // TODO
            // let config = AccountConfig { kind : AccountKind::Bip32 };
            // storage_accounts
            let rpc_api : Arc<crate::DynRpcApi> = self.rpc.clone();
            Arc::new(Account::new(rpc_api, stored))
        }).collect();
        *self.inner.accounts.lock().unwrap() = accounts;
    }

    pub fn rpc(&self) -> Arc<KaspaRpcClient> {
        self.rpc.clone()
    }

    // intended for starting async management tasks
    pub async fn start(self: &Arc<Wallet>) -> Result<()> {
        // internal event loop
        self.start_task().await?;
        // rpc services (notifier)
        self.rpc.start().await?;
        // start async RPC connection
        self.rpc.connect_as_task()?;
        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(self: &Arc<Wallet>) -> Result<()> {
        self.rpc.stop().await?;
        self.stop_task().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        self.inner.listener_id.lock().unwrap().clone()
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

    pub async fn balance(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn broadcast(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn create(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn create_unsigned_transaction(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn dump_unencrypted(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    // fn account(self: &Arc<Self>) -> Result<Arc<dyn WalletAccountTrait>> {
    //     Ok(self.inner.lock().unwrap().accounts.get(0).unwrap().clone())
    // }

    pub async fn select(&self, account : Option<Arc<Account>>) -> Result<()> {
        *self.inner.selected_account.lock().unwrap() = account;
        Ok(())
    }

    pub async fn account(self : &Arc<Self>) -> Result<Arc<Account>> {
        Ok(self.inner.selected_account.lock().unwrap().clone().ok_or_else(|| Error::AccountSelection)?)
        // let account = self.inner.selected_account.lock().unwrap().clone();
        // if let Some(account) = account {
        //     account
        // } else {
        // }
        // Ok(self.inner.accounts.lock().unwrap().get(0).unwrap().clone())
    }

    pub async fn accounts(&self) -> Vec<Arc<Account>> {
        self.inner.accounts.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    fn receive_wallet(self: &Arc<Self>) -> Result<Arc<dyn AddressGeneratorTrait>> {
        todo!()
        // Ok(self.account()?.receive_wallet())
    }
    
    fn change_wallet(self: &Arc<Self>) -> Result<Arc<dyn AddressGeneratorTrait>> {
        todo!()
        // Ok(self.account()?.change_wallet())
    }

    pub async fn new_address(self: &Arc<Self>) -> Result<String> {
        todo!()
        // let address = self.receive_wallet()?.new_address().await?;
        // Ok(address.into())
    }

    pub async fn new_change_address(self: &Arc<Self>) -> Result<String> {
        let address = self.change_wallet()?.new_address().await?;
        Ok(address.into())
    }

    pub async fn parse(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn send(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn show_address(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn sign(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn sweep(self: &Arc<Wallet>) -> Result<()> {
        Ok(())
    }

    pub async fn start_task(&self) -> Result<()> {

        let self_ = self.clone();
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
