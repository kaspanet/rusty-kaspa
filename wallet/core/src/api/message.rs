//!
//! Messages used by the Wallet API.
//!
//! Each Wallet API `xxx_call()` method has a corresponding
//! `XxxRequest` and `XxxResponse` message.
//!

use crate::imports::*;
use crate::tx::{Fees, GeneratorSummary, PaymentDestination};
use kaspa_addresses::Address;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlushRequest {
    pub wallet_secret: Secret,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlushResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub url: Option<String>,
    pub network_id: NetworkId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeNetworkIdRequest {
    pub network_id: NetworkId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeNetworkIdResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainContextRequest {
    pub name: String,
    pub data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainContextResponse {
    // pub name : String,
    // pub data: Option<Arc<Vec<u8>>>,
    // pub is_connected: bool,
    // pub is_synced: bool,
    // pub is_open: bool,
    // pub url: Option<String>,
    // pub is_wrpc_client: bool,
    // pub network_id: Option<NetworkId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStatusRequest {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStatusResponse {
    pub is_connected: bool,
    pub is_synced: bool,
    pub is_open: bool,
    pub url: Option<String>,
    pub is_wrpc_client: bool,
    pub network_id: Option<NetworkId>,
    pub context: Option<Arc<Vec<u8>>>,
    pub wallet_descriptor: Option<WalletDescriptor>,
    pub account_descriptors: Option<Vec<AccountDescriptor>>,
    pub selected_account_id: Option<AccountId>,
}

// ---

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletEnumerateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletEnumerateResponse {
    pub wallet_descriptors: Vec<WalletDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCreateRequest {
    pub wallet_secret: Secret,
    pub wallet_args: WalletCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCreateResponse {
    pub storage_descriptor: StorageDescriptor,
    pub wallet_descriptor: WalletDescriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletOpenRequest {
    pub wallet_secret: Secret,
    pub filename: Option<String>,
    pub account_descriptors: bool,
    pub legacy_accounts: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletOpenResponse {
    pub account_descriptors: Option<Vec<AccountDescriptor>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCloseRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCloseResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletReloadRequest {
    pub reactivate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletReloadResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletRenameRequest {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub wallet_secret: Secret,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletRenameResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletRenameFileResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletChangeSecretRequest {
    pub old_wallet_secret: Secret,
    pub new_wallet_secret: Secret,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletChangeSecretResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletExportRequest {
    pub wallet_secret: Secret,
    pub include_transactions: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletExportResponse {
    pub wallet_data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletImportRequest {
    pub wallet_secret: Secret,
    pub wallet_data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletImportResponse {
    pub wallet_descriptor: WalletDescriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataEnumerateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataEnumerateResponse {
    pub prv_key_data_list: Vec<Arc<PrvKeyDataInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataCreateRequest {
    pub wallet_secret: Secret,
    pub prv_key_data_args: PrvKeyDataCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataCreateResponse {
    pub prv_key_data_id: PrvKeyDataId,
}

// TODO
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataRemoveRequest {
    pub wallet_secret: Secret,
    pub prv_key_data_id: PrvKeyDataId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataRemoveResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataGetRequest {
    pub wallet_secret: Secret,
    pub prv_key_data_id: PrvKeyDataId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataGetResponse {
    pub prv_key_data: Option<PrvKeyData>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsEnumerateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsEnumerateResponse {
    pub account_descriptors: Vec<AccountDescriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsRenameRequest {
    pub account_id: AccountId,
    pub name: Option<String>,
    pub wallet_secret: Secret,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsRenameResponse {}

/// @category Wallet API
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen]
pub enum AccountsDiscoveryKind {
    Bip44,
}

impl FromStr for AccountsDiscoveryKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "bip44" => Ok(Self::Bip44),
            _ => Err(Error::custom(format!("Invalid discovery kind: {s}"))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsDiscoveryRequest {
    pub discovery_kind: AccountsDiscoveryKind,
    pub address_scan_extent: u32,
    pub account_scan_extent: u32,
    pub bip39_passphrase: Option<Secret>,
    pub bip39_mnemonic: Secret,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsDiscoveryResponse {
    pub last_account_index_found: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCreateRequest {
    pub wallet_secret: Secret,
    pub account_create_args: AccountCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCreateResponse {
    pub account_descriptor: AccountDescriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsEnsureDefaultRequest {
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub account_kind: AccountKind,
    pub mnemonic_phrase: Option<Secret>,
    // pub account_create_args: AccountCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsEnsureDefaultResponse {
    pub account_descriptor: AccountDescriptor,
}

// TODO
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsImportRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsImportResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsSelectRequest {
    pub account_id: Option<AccountId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsSelectResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsActivateRequest {
    pub account_ids: Option<Vec<AccountId>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsActivateResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsDeactivateRequest {
    pub account_ids: Option<Vec<AccountId>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsDeactivateResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsGetRequest {
    pub account_id: AccountId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsGetResponse {
    pub account_descriptor: AccountDescriptor,
}

/// Specifies the type of an account address to create.
/// The address can bea receive address or a change address.
///
/// @category Wallet API
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "wasm32-sdk", wasm_bindgen)]
pub enum NewAddressKind {
    Receive,
    Change,
}

impl FromStr for NewAddressKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "receive" => Ok(Self::Receive),
            "change" => Ok(Self::Change),
            _ => Err(Error::custom(format!("Invalid address kind: {s}"))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCreateNewAddressRequest {
    pub account_id: AccountId,
    #[serde(rename = "type")]
    pub kind: NewAddressKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCreateNewAddressResponse {
    pub address: Address,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsSendRequest {
    pub account_id: AccountId,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub destination: PaymentDestination,
    pub priority_fee_sompi: Fees,
    pub payload: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsSendResponse {
    pub generator_summary: GeneratorSummary,
    pub transaction_ids: Vec<TransactionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsTransferRequest {
    pub source_account_id: AccountId,
    pub destination_account_id: AccountId,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub transfer_amount_sompi: u64,
    pub priority_fee_sompi: Option<Fees>,
    // pub priority_fee_sompi: Fees,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsTransferResponse {
    pub generator_summary: GeneratorSummary,
    pub transaction_ids: Vec<TransactionId>,
}

// TODO: Use Generator Summary from WASM module...

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsEstimateRequest {
    pub account_id: AccountId,
    pub destination: PaymentDestination,
    pub priority_fee_sompi: Fees,
    pub payload: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsEstimateResponse {
    pub generator_summary: GeneratorSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsDataGetRequest {
    pub account_id: AccountId,
    pub network_id: NetworkId,
    pub filter: Option<Vec<TransactionKind>>,
    pub start: u64,
    pub end: u64,
}

impl TransactionsDataGetRequest {
    pub fn with_range(account_id: AccountId, network_id: NetworkId, range: std::ops::Range<u64>) -> Self {
        Self { account_id, network_id, filter: None, start: range.start, end: range.end }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsDataGetResponse {
    pub account_id: AccountId,
    pub transactions: Vec<Arc<TransactionRecord>>,
    pub start: u64,
    pub total: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsReplaceNoteRequest {
    pub account_id: AccountId,
    pub network_id: NetworkId,
    pub transaction_id: TransactionId,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsReplaceNoteResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsReplaceMetadataRequest {
    pub account_id: AccountId,
    pub network_id: NetworkId,
    pub transaction_id: TransactionId,
    pub metadata: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsReplaceMetadataResponse {}

// #[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct TransactionGetRequest {}

// #[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
// #[serde(rename_all = "camelCase")]
// pub struct TransactionGetResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressBookEnumerateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressBookEnumerateResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletNotification {}
