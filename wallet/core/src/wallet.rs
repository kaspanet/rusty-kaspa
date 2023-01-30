use crate::result::Result;
use crate::wallets::HDWalletGen1;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use rpc_core::{api::rpc::RpcApi, prelude::ListenerID as ListenerId, NotificationMessage, NotificationType};
use std::sync::{Arc, Mutex};
use workflow_core::channel::{Channel, Receiver};

#[derive(Clone)]
pub struct Wallet {
    rpc: Arc<KaspaRpcClient>,
    hd_wallet: HDWalletGen1,
    listener_id: Arc<Mutex<Option<ListenerId>>>,
    notification_channel: Channel<Arc<NotificationMessage>>,
}

impl Wallet {
    pub async fn try_new() -> Result<Wallet> {
        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let wallet = Wallet {
            rpc: Arc::new(KaspaRpcClient::new(WrpcEncoding::Borsh, "wrpc://localhost:17110")?),
            hd_wallet: HDWalletGen1::from_master_xprv(master_xprv, false, 0).await?,
            notification_channel: Channel::unbounded(),
            listener_id: Arc::new(Mutex::new(None)),
        };

        Ok(wallet)
    }

    pub fn rpc(&self) -> Arc<KaspaRpcClient> {
        self.rpc.clone()
    }

    pub fn notification_channel_receiver(&self) -> Receiver<Arc<NotificationMessage>> {
        self.notification_channel.receiver.clone()
    }

    // intended for starting async management tasks
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        self.rpc.start()?;
        // self.rpc.connect_as_task()?;
        let id = self.rpc.register_new_listener(self.notification_channel.sender.clone());
        *self.listener_id.lock().unwrap() = Some(id);
        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.rpc.stop().await?;
        Ok(())
    }

    pub fn listener_id(&self) -> ListenerId {
        self.listener_id.lock().unwrap().unwrap_or_else(|| panic!("Wallet is missing notification `listener_id`"))
    }

    // ~~~

    pub async fn info(&self) -> Result<String> {
        let v = self.rpc.get_info().await?;
        Ok(format!("{v:#?}").replace('\n', "\r\n"))
    }

    pub async fn subscribe_daa_score(&self) -> Result<()> {
        self.rpc.start_notify(self.listener_id(), NotificationType::VirtualDaaScoreChanged).await?;
        Ok(())
    }

    pub async fn unsubscribe_daa_score(&self) -> Result<()> {
        self.rpc.stop_notify(self.listener_id(), NotificationType::VirtualDaaScoreChanged).await?;
        Ok(())
    }

    pub async fn ping(&self, _msg: String) -> Result<String> {
        // let rpc: Arc<dyn ClientInterface> = self.rpc.clone();
        // let resp = self.rpc.ping(msg).await?;
        // Ok(resp)
        //let address =self.hd_wallet.receive_wallet().derive_address(0).await?;
        //Ok(address.into())
        Ok("not implemented".to_string())
    }

    pub async fn balance(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn broadcast(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn create(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn create_unsigned_transaction(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn dump_unencrypted(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn new_address(self: &Arc<Self>) -> Result<String> {
        let address = self.hd_wallet.receive_wallet().new_address().await?;
        Ok(address.into())
        //Ok("new_address".to_string())
    }

    pub async fn parse(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn send(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn show_address(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn sign(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    pub async fn sweep(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }
}
