#![allow(unused_imports)]

pub use addresses::{Address,Version as AddressVersion};
pub use consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
pub use consensus_core::wasm::keypair::{Keypair, PrivateKey};

pub mod rpc {
    //! Kaspa RPC interface
    pub use kaspa_rpc_core::model::message::*;
    pub use kaspa_wrpc_client::wasm::{
        RpcClient,
    };
}

pub use kaspa_wallet_core::{account::Account, signer::{Signer,

js_sign_transaction as sign_transaction
}, wallet::Wallet, storage::Store, utxo::UtxoSet };
