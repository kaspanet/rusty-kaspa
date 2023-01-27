use crate::result::Result;
use crate::wallets::HDWalletGen1;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use std::sync::Arc;

#[derive(Clone)]
pub struct Wallet {
    rpc: Arc<KaspaRpcClient>,
    hd_wallet: HDWalletGen1,
}

impl Wallet {
    pub async fn try_new() -> Result<Wallet> {
        let master_xprv =
            "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ";

        let wallet = Wallet {
            rpc: Arc::new(KaspaRpcClient::new(WrpcEncoding::Borsh, "wrpc://localhost:9292")?),
            hd_wallet: HDWalletGen1::from_master_xprv(master_xprv, false, 0).await?,
        };

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
