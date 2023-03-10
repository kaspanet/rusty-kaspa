use crate::result::Result;
use crate::wallets::HDWalletGen1;
use kaspa_notify::listener::ListenerId;
use kaspa_rpc_core::{api::rpc::RpcApi, Notification};
use kaspa_wrpc_client::{KaspaRpcClient, NotificationMode, WrpcEncoding};
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use workflow_core::channel::{Channel, Receiver};

#[derive(Clone)]
pub struct Wallet {
    pub rpc: Arc<KaspaRpcClient>,
    hd_wallet: HDWalletGen1,
    listener_id: Arc<Mutex<Option<ListenerId>>>,
    _notification_channel: Channel<Arc<Notification>>,
    notification_mode: NotificationMode,
}

impl Wallet {
    pub async fn try_new() -> Result<Wallet> {
        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let notification_mode = NotificationMode::Direct;
        let wallet = Wallet {
            rpc: Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, notification_mode.clone(), "wrpc://localhost:17110")?),
            hd_wallet: HDWalletGen1::from_master_xprv(master_xprv, false, 0).await?,
            _notification_channel: Channel::unbounded(),
            listener_id: Arc::new(Mutex::new(None)),
            notification_mode,
        };

        Ok(wallet)
    }

    pub fn rpc(&self) -> Arc<KaspaRpcClient> {
        self.rpc.clone()
    }

    // pub fn notification_channel_receiver(&self) -> Receiver<Arc<NotificationMessage>> {
    //     match self.notification_mode {
    //         // NotificationMode::NotSynced => self.notification_channel.receiver.clone(),
    //         NotificationMode::Direct => self.rpc.notification_channel_receiver(),
    //     }
    //     self.notification_channel.receiver.clone()
    // }

    // intended for starting async management tasks
    pub async fn start(self: &Arc<Wallet>) -> Result<()> {
        self.rpc.start().await?;
        self.rpc.connect_as_task()?;

        //
        // FIXME
        //

        // // TODO - this won't work if implementing NotificationMode::Synced
        // if matches!(self.notification_mode, NotificationMode::NotSynced) {
        //     let id = self.rpc.register_new_listener(self.notification_channel.sender.clone());
        //     *self.listener_id.lock().unwrap() = Some(id);
        // }
        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(self: &Arc<Wallet>) -> Result<()> {
        self.rpc.stop().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        match &self.notification_mode {
            NotificationMode::NotSynced => self
                .listener_id
                .lock()
                .unwrap()
                .unwrap_or_else(|| panic!("Wallet::listener_id is not present for `NotificationMode::NotSynced`")),
            NotificationMode::Direct => ListenerId::default(),
        }
    }

    // ~~~

    pub async fn get_info(&self) -> Result<String> {
        let v = self.rpc.get_info().await?;
        Ok(format!("{v:#?}").replace('\n', "\r\n"))
    }

    //
    // FIXME
    //

    // pub async fn subscribe_daa_score(&self) -> Result<()> {
    //     self.rpc.start_notify(self.listener_id(), Scope::VirtualDaaScoreChanged).await?;
    //     Ok(())
    // }

    // pub async fn unsubscribe_daa_score(&self) -> Result<()> {
    //     self.rpc.stop_notify(self.listener_id(), Scope::VirtualDaaScoreChanged).await?;
    //     Ok(())
    // }

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

    pub async fn new_address(self: &Arc<Wallet>) -> Result<String> {
        let address = self.hd_wallet.receive_wallet().new_address().await?;
        Ok(address.into())
        //Ok("new_address".to_string())
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
}
