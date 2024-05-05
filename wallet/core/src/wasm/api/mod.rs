use crate::api::message::*;
use crate::api::traits::*;
use crate::imports::*;
use crate::result::Result;
use kaspa_wallet_macros::declare_wasm_handlers;

pub mod extensions;
pub mod message;

use self::message::*;

use super::Wallet;

declare_wasm_handlers!([
    /// Ping backend
    // Ping,
    Batch,
    Flush,
    // Connect,
    // Disconnect,
    RetainContext,
    GetStatus,
    WalletEnumerate,
    WalletCreate,
    WalletOpen,
    WalletReload,
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
    AccountsEnsureDefault,
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
