extern crate alloc;
extern crate self as kaspa_wallet_core;

pub mod account;
pub mod accounts;
pub mod address;
pub mod convert;
pub mod encryption;
pub mod error;
pub mod imports;
pub mod keypair;
pub mod result;
pub mod secret;
pub mod signer;
pub mod storage;
pub mod tx;
pub mod utils;
pub mod utxo;
pub mod wallet;
pub mod xprivatekey;
pub mod xpublickey;

pub use accounts::dummy_address;
pub use address::AddressDerivationManager;
pub use kaspa_addresses::Address;
pub use result::Result;
pub use signer::Signer;
pub use wallet::Wallet;
pub use xprivatekey::XPrivateKey;
pub use xpublickey::XPublicKey;

#[macro_export]
macro_rules! hex {
    ($str: literal) => {{
        let len = $str.as_bytes().len() / 2;
        let mut dst = vec![0; len];
        dst.resize(len, 0);
        faster_hex::hex_decode_fallback($str.as_bytes(), &mut dst);
        dst
    }
    [..]};
}

use kaspa_rpc_core::Notification;
use kaspa_utils::channel::Channel;

pub type DynRpcApi = dyn kaspa_rpc_core::api::rpc::RpcApi;
pub type NotificationChannel = Channel<Notification>;
