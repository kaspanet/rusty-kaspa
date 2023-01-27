use crate::*;
use std::sync::Arc;
use async_trait::async_trait;

pub struct WalletGen1 {}

impl WalletGen1 {
    async fn open_wallet_impl(_encrypted_wallet: &str, _password: &str) -> Result<Arc<Self>> {
        let wallet = Arc::new(Self {});

        Ok(wallet)
    }
}

#[async_trait]
impl WalletWrapper for WalletGen1 {
    async fn open_wallet(encrypted_wallet: &str, password: &str) -> Result<Arc<Self>> {
        let wallet = Self::open_wallet_impl(encrypted_wallet, password).await?;
        Ok(wallet)
    }
    async fn sync(&self) -> Result<()> {
        Ok(())
    }
    async fn receive_address(&self) -> Result<Address> {
        Ok(dummy_address())
    }
}
