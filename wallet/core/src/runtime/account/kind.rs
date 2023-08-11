#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::address::{build_derivate_paths, AddressManager};
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{AtomicBalance, Balance, BalanceStrings, Wallet};
use crate::secret::Secret;
use crate::storage::interface::AccessContext;
use crate::storage::{self, AccessContextT, PrvKeyData, PrvKeyDataId, PubKeyData};
use crate::tx::{Fees, Generator, GeneratorSettings, GeneratorSummary, KeydataSigner, PaymentDestination, PendingTransaction, Signer};
use crate::utxo::{UtxoContext, UtxoContextBinding, UtxoEntryReference};
use crate::AddressDerivationManager;
use faster_hex::hex_string;
use futures::future::join_all;
use kaspa_bip32::ChildNumber;
use kaspa_notify::listener::ListenerId;
use secp256k1::{ONE_KEY, PublicKey, SecretKey};
use separator::Separatable;
use serde::Serializer;
use std::hash::Hash;
use std::str::FromStr;
use workflow_core::abortable::Abortable;
use workflow_core::enums::u8_try_from;
use kaspa_addresses::Version as AddressVersion;

u8_try_from! {
    #[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Hash)]
    #[serde(rename_all = "lowercase")]
    #[wasm_bindgen]
    pub enum AccountKind {
        Legacy,
        #[default]
        Bip32,
        MultiSig,
        Secp256k1Keypair,
        Resident,
    }
}

impl std::fmt::Display for AccountKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountKind::Legacy => write!(f, "legacy"),
            AccountKind::Bip32 => write!(f, "bip32"),
            AccountKind::MultiSig => write!(f, "multisig"),
            AccountKind::Secp256k1Keypair => write!(f, "secp256k1keypair"),
            AccountKind::Resident => write!(f, "resident"),
        }
    }
}

impl FromStr for AccountKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "legacy" => Ok(AccountKind::Legacy),
            "bip32" => Ok(AccountKind::Bip32),
            "multisig" => Ok(AccountKind::MultiSig),
            "secp256k1keypair" => Ok(AccountKind::Secp256k1Keypair),
            "resident" => Ok(AccountKind::Resident),
            _ => Err(Error::InvalidAccountKind),
        }
    }
}
