use crate::result::Result;
use crate::wallets::HDWalletGen1;
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
use std::sync::Arc;
#[allow(unused_imports)]
use workflow_core::channel::{Channel, Receiver};

#[derive(Clone)]
pub struct Wallet {
    pub rpc: Arc<KaspaRpcClient>,
    hd_wallet: HDWalletGen1,
    listener_id: ListenerId,
    notification_receiver: Receiver<Notification>,
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
            Arc::new(KaspaRpcClient::new_with_args(WrpcEncoding::Borsh, NotificationMode::Direct, "wrpc://localhost:17110")?)
        };

        let (listener_id, notification_receiver) = match rpc.notification_mode() {
            NotificationMode::MultiListeners => {
                let notification_channel = Channel::unbounded();
                let connection = ChannelConnection::new(notification_channel.sender);
                (rpc.register_new_listener(connection), notification_channel.receiver)
            }
            NotificationMode::Direct => (ListenerId::default(), rpc.notification_channel_receiver()),
        };

        let wallet = Wallet {
            rpc,
            hd_wallet: HDWalletGen1::from_master_xprv(master_xprv, false, 0).await?,
            notification_receiver,
            listener_id,
        };

        Ok(wallet)
    }

    pub fn rpc(&self) -> Arc<KaspaRpcClient> {
        self.rpc.clone()
    }

    // intended for starting async management tasks
    pub async fn start(self: &Arc<Wallet>) -> Result<()> {
        self.rpc.start().await?;
        self.rpc.connect_as_task()?;
        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(self: &Arc<Wallet>) -> Result<()> {
        self.rpc.stop().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        self.listener_id
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Notification> {
        self.notification_receiver.clone()
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
