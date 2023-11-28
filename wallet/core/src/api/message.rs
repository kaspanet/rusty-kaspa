use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use kaspa_consensus_core::{network::NetworkId, tx::TransactionId};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use workflow_rpc::id::{Generator, Id64 as TaskId};

use crate::{
    runtime::{account::descriptor::AccountDescriptor, AccountCreateArgs, PrvKeyDataCreateArgs, WalletCreateArgs},
    secret::Secret,
    storage::{AccountId, PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, TransactionRecord, TransactionType, WalletDescriptor},
    tx::{Fees, GeneratorSummary, PaymentDestination},
};
// use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {
    pub v: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {
    pub v: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSettingsGetRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSettingsGetResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSettingsSetRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSettingsSetResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatusRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatusResponse {}

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
    pub wallet_args: WalletCreateArgs,
    pub prv_key_data_args: PrvKeyDataCreateArgs,
    pub account_args: AccountCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCreateResponse {
    pub mnemonic: String,
    pub wallet_descriptor: Option<String>,
    pub account_descriptor: AccountDescriptor,
    // pub account_id : AccountId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletOpenRequest {
    pub wallet_secret: Secret,
    pub wallet_name: Option<String>,
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
pub struct PrvKeyDataEnumerateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataEnumerateResponse {
    pub prv_key_data_list: Vec<Arc<PrvKeyDataInfo>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataCreateRequest {
    pub prv_key_data_args: PrvKeyDataCreateArgs,
    pub fetch_mnemonic: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataCreateResponse {
    pub prv_key_data_id: PrvKeyDataId,
    pub mnemonic: Option<String>,
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
pub struct AccountsCreateRequest {
    pub prv_key_data_id: PrvKeyDataId,
    pub account_args: AccountCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCreateResponse {
    pub descriptor: AccountDescriptor,
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
    pub task_id: Option<TaskId>,
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
pub struct TransactionDataGetRequest {
    pub account_id: AccountId,
    pub network_id: NetworkId,
    pub filter: Option<Vec<TransactionType>>,
    pub start: u64,
    pub end: u64,
}

impl TransactionDataGetRequest {
    pub fn with_range(account_id: AccountId, network_id: NetworkId, range: std::ops::Range<u64>) -> Self {
        Self { account_id, network_id, filter: None, start: range.start, end: range.end }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDataGetResponse {
    pub account_id: AccountId,
    pub transactions: Vec<Arc<TransactionRecord>>,
    pub start: u64,
    pub total: u64,
}

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
