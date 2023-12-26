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
    pub payload: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub payload: Option<u64>,
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
    pub url: String,
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
pub struct GetStatusRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetStatusResponse {
    pub is_connected: bool,
    pub is_synced: bool,
    pub is_open: bool,
    pub url: Option<String>,
    pub is_wrpc_client: bool,
    pub network_id: Option<NetworkId>,
}

// ---

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletEnumerateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletEnumerateResponse {
    pub wallet_list: Vec<WalletDescriptor>,
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
    pub wallet_filename: Option<String>,
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

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataRemoveRequest {}

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
    pub descriptor_list: Vec<AccountDescriptor>,
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

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountsDiscoveryKind {
    Bip44,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsDiscoveryRequest {
    pub discovery_kind: AccountsDiscoveryKind,
    pub address_scan_extent: u32,
    pub account_scan_extent: u32,
    pub bip39_passphrase: Option<Secret>,
    pub bip39_mnemonic: String,
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
pub struct AccountsImportRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsImportResponse {}

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
    pub descriptor: AccountDescriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewAddressKind {
    Receive,
    Change,
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
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsTransferResponse {
    pub generator_summary: GeneratorSummary,
    pub transaction_ids: Vec<TransactionId>,
}

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
