//!
//! Structs used as various arguments for internal wallet operations.
//!

use crate::imports::*;
// use crate::secret::Secret;
use crate::storage::interface::CreateArgs;
use crate::storage::{Hint, PrvKeyDataId};
use borsh::{BorshDeserialize, BorshSerialize};
use zeroize::Zeroize;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCreateArgs {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub encryption_kind: EncryptionKind,
    pub user_hint: Option<Hint>,
    pub overwrite_wallet_storage: bool,
}

impl WalletCreateArgs {
    pub fn new(
        title: Option<String>,
        filename: Option<String>,
        encryption_kind: EncryptionKind,
        user_hint: Option<Hint>,
        overwrite_wallet_storage: bool,
    ) -> Self {
        Self { title, filename, encryption_kind, user_hint, overwrite_wallet_storage }
    }
}

impl From<WalletCreateArgs> for CreateArgs {
    fn from(args: WalletCreateArgs) -> Self {
        CreateArgs::new(args.title, args.filename, args.encryption_kind, args.user_hint, args.overwrite_wallet_storage)
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct WalletOpenArgs {
    /// Return account descriptors
    pub account_descriptors: bool,
    /// Enable support for legacy accounts
    pub legacy_accounts: bool,
}

impl WalletOpenArgs {
    pub fn default_with_legacy_accounts() -> Self {
        Self { legacy_accounts: true, ..Default::default() }
    }

    pub fn load_account_descriptors(&self) -> bool {
        self.account_descriptors || self.legacy_accounts
    }

    pub fn is_legacy_only(&self) -> bool {
        self.legacy_accounts && !self.account_descriptors
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PrvKeyDataCreateArgs {
    pub name: Option<String>,
    pub payment_secret: Option<Secret>,
    pub mnemonic: Secret,
}

impl PrvKeyDataCreateArgs {
    pub fn new(name: Option<String>, payment_secret: Option<Secret>, mnemonic: Secret) -> Self {
        Self { name, payment_secret, mnemonic }
    }
}

impl Zeroize for PrvKeyDataCreateArgs {
    fn zeroize(&mut self) {
        self.mnemonic.zeroize();
    }
}

#[wasm_bindgen(typescript_custom_section)]
const TS_ACCOUNT_CREATE_ARGS: &'static str = r#"

export interface IPrvKeyDataArgs {
    prvKeyDataId: HexString;
    paymentSecret?: string;
}

export interface IAccountCreateArgsBip32 {
    accountName?: string;
    accountIndex?: number;
}

/**
 * @category Wallet API
 */
export interface IAccountCreateArgs {
    type : "bip32";
    args : IAccountCreateArgsBip32;
    prvKeyDataArgs? : IPrvKeyDataArgs;
}
"#;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountCreateArgsBip32 {
    pub account_name: Option<String>,
    pub account_index: Option<u64>,
}

impl AccountCreateArgsBip32 {
    pub fn new(account_name: Option<String>, account_index: Option<u64>) -> Self {
        Self { account_name, account_index }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PrvKeyDataArgs {
    pub prv_key_data_id: PrvKeyDataId,
    pub payment_secret: Option<Secret>,
}

impl PrvKeyDataArgs {
    pub fn new(prv_key_data_id: PrvKeyDataId, payment_secret: Option<Secret>) -> Self {
        Self { prv_key_data_id, payment_secret }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", content = "args")]
pub enum AccountCreateArgs {
    Bip32 {
        prv_key_data_args: PrvKeyDataArgs,
        account_args: AccountCreateArgsBip32,
    },
    Legacy {
        prv_key_data_id: PrvKeyDataId,
        account_name: Option<String>,
    },
    Multisig {
        prv_key_data_args: Vec<PrvKeyDataArgs>,
        additional_xpub_keys: Vec<String>,
        name: Option<String>,
        minimum_signatures: u16,
    },
}

impl AccountCreateArgs {
    pub fn new_bip32(
        prv_key_data_id: PrvKeyDataId,
        payment_secret: Option<Secret>,
        account_name: Option<String>,
        account_index: Option<u64>,
    ) -> Self {
        let prv_key_data_args = PrvKeyDataArgs { prv_key_data_id, payment_secret };
        let account_args = AccountCreateArgsBip32 { account_name, account_index };
        AccountCreateArgs::Bip32 { prv_key_data_args, account_args }
    }

    pub fn new_legacy(prv_key_data_id: PrvKeyDataId, account_name: Option<String>) -> Self {
        AccountCreateArgs::Legacy { prv_key_data_id, account_name }
    }

    pub fn new_multisig(
        prv_key_data_args: Vec<PrvKeyDataArgs>,
        additional_xpub_keys: Vec<String>,
        name: Option<String>,
        minimum_signatures: u16,
    ) -> Self {
        AccountCreateArgs::Multisig { prv_key_data_args, additional_xpub_keys, name, minimum_signatures }
    }
}
