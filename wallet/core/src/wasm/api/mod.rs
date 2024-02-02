use crate::api::message::*;
use crate::api::traits::*;
use kaspa_wallet_macros::declare_wasm_handlers;
// use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
// use crate::secret::Secret;
// use crate::storage::{PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, WalletDescriptor};
// use crate::tx::GeneratorSummary;
// use workflow_core::channel::Receiver;

pub mod extensions;
pub mod message;

use self::message::*;

use super::Wallet;

declare_wasm_handlers!([
    Ping,
    Batch,
    Flush,
    // Connect,
    // Disconnect,
    GetStatus,
    WalletEnumerate,
    WalletCreate,
    WalletOpen,
    WalletClose,
    // WalletExists,
    // WalletRename,
    WalletChangeSecret,
    WalletExport,
    WalletImport,
    PrvKeyDataEnumerate,
    PrvKeyDataCreate,
    PrvKeyDataRemove,
    PrvKeyDataGet,
    AccountsEnumerate,
    AccountsRename,
    AccountsDiscovery,
    AccountsCreate,
    AccountsImport,
    AccountsActivate,
    AccountsDeactivate,
    // AccountsRemove,
    AccountsGet,
    AccountsCreateNewAddress,
    AccountsSend,
    AccountsTransfer,
    AccountsEstimate,
    TransactionsDataGet,
    TransactionsReplaceNote,
    TransactionsReplaceMetadata,
    AddressBookEnumerate,
]);
