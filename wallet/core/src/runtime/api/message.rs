use std::sync::Arc;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use kaspa_consensus_core::{network::NetworkId, tx::TransactionId};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use workflow_rpc::id::{Generator, Id64 as TaskId};

use crate::{
    runtime::{account::descriptor::Descriptor, AccountCreateArgs, PrvKeyDataCreateArgs, WalletCreateArgs},
    secret::Secret,
    storage::{AccountId, PrvKeyData, PrvKeyDataId, TransactionRecord, TransactionType, WalletDescriptor},
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
    pub descriptor: Option<String>,
    // pub account_id : AccountId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletOpenRequest {
    pub wallet_secret: Secret,
    pub wallet_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletOpenResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCloseRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCloseResponse {}

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
pub struct AccountEnumerateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountEnumerateResponse {
    pub descriptors: Vec<Descriptor>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCreateRequest {
    pub prv_key_data_id: PrvKeyDataId,
    pub account_args: AccountCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCreateResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountImportRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountImportResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountGetRequest {
    pub account_id: AccountId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountGetResponse {
    pub descriptor: Descriptor,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCreateNewAddressRequest {
    pub account_id: AccountId,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountCreateNewAddressResponse {
    pub address: Address,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSendRequest {
    pub task_id: Option<TaskId>,
    pub account_id: AccountId,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub destination: PaymentDestination,
    pub priority_fee_sompi: Fees,
    pub payload: Option<Vec<u8>>,
    // abortable: &Abortable,
    // notifier: Option<GenerationNotifier>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSendResponse {
    pub generator_summary: GeneratorSummary,
    pub transaction_ids: Vec<TransactionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountEstimateRequest {
    pub task_id: Option<TaskId>,
    pub account_id: AccountId,
    pub destination: PaymentDestination,
    pub priority_fee_sompi: Fees,
    pub payload: Option<Vec<u8>>,
    // abortable: &Abortable,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountEstimateResponse {
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

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDataGetResponse {
    pub transactions: Vec<Arc<TransactionRecord>>,
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
