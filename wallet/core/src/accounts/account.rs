use crate::Result;
use async_trait::async_trait;
use kaspa_bip32::ExtendedPublicKey;
use std::sync::Arc;

#[async_trait]
pub trait WalletDerivationManagerTrait: Send + Sync {
    async fn from_master_xprv(xprv: &str, is_multisig: bool, account_index: u64, cosigner_index: Option<u32>) -> Result<Self>
    where
        Self: Sized;

    async fn from_extended_public_key_str(
        xpub: &str, //xpub is drived upto m/<purpose>'/<CoinType>'/<account_index>'
        cosigner_index: Option<u32>,
    ) -> Result<Self>
    where
        Self: Sized;

    async fn from_extended_public_key(
        extended_public_key: ExtendedPublicKey<secp256k1::PublicKey>,
        cosigner_index: Option<u32>,
    ) -> Result<Self>
    where
        Self: Sized;

    fn receive_pubkey_manager(&self) -> Arc<dyn PubkeyDerivationManagerTrait>;
    fn change_pubkey_manager(&self) -> Arc<dyn PubkeyDerivationManagerTrait>;

    async fn receive_pubkey(&self) -> Result<secp256k1::PublicKey>;
    async fn change_pubkey(&self) -> Result<secp256k1::PublicKey>;

    async fn derive_receive_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey>;
    async fn derive_change_pubkey(&self, index: u32) -> Result<secp256k1::PublicKey>;

    async fn new_receive_pubkey(&self) -> Result<secp256k1::PublicKey>;
    async fn new_change_pubkey(&self) -> Result<secp256k1::PublicKey>;
}

#[async_trait]
pub trait PubkeyDerivationManagerTrait: Send + Sync {
    async fn new_pubkey(&self) -> Result<secp256k1::PublicKey>;
    fn current_pubkey(&self) -> Result<secp256k1::PublicKey>;
    fn index(&self) -> Result<u32>;
    fn set_index(&self, index: u32) -> Result<()>;
    async fn get_range(&self, range: std::ops::Range<u32>) -> Result<Vec<secp256k1::PublicKey>>;
}
