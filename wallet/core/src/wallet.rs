use crate::result::Result;
// use rpc_core::client::prelude::*;
use kaspa_wrpc_client::{KaspaRpcClient,WrpcEncoding};
use rpc_core::api::rpc::RpcApi;
use std::sync::Arc;
// use kaspa_rpc_core::client::prelude::*;
// use workflow_log::*;

#[derive(Clone)]
pub struct Wallet {
    rpc: Arc<KaspaRpcClient>,
}

impl Wallet {
    pub fn try_new() -> Result<Wallet> {
        let wallet = Wallet { rpc: Arc::new(KaspaRpcClient::new(WrpcEncoding::Borsh,"wrpc://localhost:9292")?) };

        Ok(wallet)
    }

    // intended for starting async management task
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        // log_info!("Wallet starting...");

        // self.rpc.connect(true).await;

        self.rpc.connect_as_task()?;

        Ok(())
    }

    // intended for stopping async management task
    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        Ok(())
    }

    // ~~~

    pub async fn info(&self) -> Result<String> {
        todo!()
        // let rpc: Arc<dyn ClientInterface> = self.rpc.clone();
        // let v = self.rpc.get_info().await?;
        // Ok(format!("{:#?}", v).replace('\n', "\r\n"))
        // let resp = self.rpc.ping(msg).await?;
        // Ok(resp)
        // Ok("not implemented".to_string())
    }

    pub async fn ping(&self, _msg: String) -> Result<String> {
        // let rpc: Arc<dyn ClientInterface> = self.rpc.clone();
        // let resp = self.rpc.ping(msg).await?;
        // Ok(resp)
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

    pub async fn new_address(self: &Arc<Self>) -> Result<()> {
        Ok(())
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
