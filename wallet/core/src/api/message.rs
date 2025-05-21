//!
//! Messages used by the Wallet API.
//!
//! Each Wallet API `xxx_call()` method has a corresponding
//! `XxxRequest` and `XxxResponse` message.
//!

use crate::imports::*;
use crate::tx::{Fees, GeneratorSummary, PaymentDestination};
use kaspa_addresses::Address;
use kaspa_consensus_client::{TransactionOutpoint, UtxoEntry};
use kaspa_rpc_core::RpcFeerateBucket;

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
    // retry on error, otherwise give up
    pub retry_on_error: bool,
    // block async call until connected, otherwise return immediately
    // and continue attempting to connect in the background
    pub block_async_connect: bool,
    // require node to be synced, fail otherwise
    pub require_sync: bool,
}

impl Default for ConnectRequest {
    fn default() -> Self {
        Self {
            url: None,
            network_id: NetworkId::new(NetworkType::Mainnet),
            retry_on_error: true,
            block_async_connect: true,
            require_sync: true,
        }
    }
}

impl ConnectRequest {
    pub fn with_url(self, url: Option<String>) -> Self {
        ConnectRequest { url, ..self }
    }

    pub fn with_network_id(self, network_id: &NetworkId) -> Self {
        ConnectRequest { network_id: *network_id, ..self }
    }

    pub fn with_retry_on_error(self, retry_on_error: bool) -> Self {
        ConnectRequest { retry_on_error, ..self }
    }

    pub fn with_block_async_connect(self, block_async_connect: bool) -> Self {
        ConnectRequest { block_async_connect, ..self }
    }

    pub fn with_require_sync(self, require_sync: bool) -> Self {
        ConnectRequest { require_sync, ..self }
    }
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
pub struct RetainContextResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetContextRequest {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetContextResponse {
    pub data: Option<Vec<u8>>,
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
pub struct AccountsImportRequest {
    pub wallet_secret: Secret,
    pub account_create_args: AccountCreateArgs,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsImportResponse {
    pub account_descriptor: AccountDescriptor,
}

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
    pub fee_rate: Option<f64>,
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
pub struct AccountsPskbSignRequest {
    pub account_id: AccountId,
    pub pskb: String,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub sign_for_address: Option<Address>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsPskbSignResponse {
    pub pskb: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsPskbBroadcastRequest {
    pub account_id: AccountId,
    pub pskb: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsPskbBroadcastResponse {
    pub transaction_ids: Vec<TransactionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsPskbSendRequest {
    pub account_id: AccountId,
    pub pskb: String,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub sign_for_address: Option<Address>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsPskbSendResponse {
    pub transaction_ids: Vec<TransactionId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsGetUtxosRequest {
    pub account_id: AccountId,
    pub addresses: Option<Vec<Address>>,
    pub min_amount_sompi: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsGetUtxosResponse {
    pub utxos: Vec<UtxoEntryWrapper>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtxoEntryWrapper {
    pub address: Option<Address>,
    pub outpoint: TransactionOutpointWrapper,
    pub amount: u64,
    pub script_public_key: ScriptPublicKey,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}
impl UtxoEntryWrapper {
    pub fn to_js_object(&self) -> Result<js_sys::Object> {
        let obj = js_sys::Object::new();
        if let Some(address) = &self.address {
            obj.set("address", &address.to_string().into())?;
        }

        let outpoint = js_sys::Object::new();
        outpoint.set("transactionId", &self.outpoint.transaction_id.to_string().into())?;
        outpoint.set("index", &self.outpoint.index.into())?;

        obj.set("amount", &self.amount.to_string().into())?;
        obj.set("outpoint", &outpoint.into())?;
        obj.set("scriptPublicKey", &workflow_wasm::serde::to_value(&self.script_public_key)?)?;
        obj.set("blockDaaScore", &self.block_daa_score.to_string().into())?;
        obj.set("isCoinbase", &self.is_coinbase.into())?;

        Ok(obj)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutpointWrapper {
    pub transaction_id: TransactionId,
    pub index: TransactionIndexType,
}

impl From<TransactionOutpoint> for TransactionOutpointWrapper {
    fn from(outpoint: TransactionOutpoint) -> Self {
        Self { transaction_id: outpoint.transaction_id(), index: outpoint.index() }
    }
}

impl From<TransactionOutpointWrapper> for TransactionOutpoint {
    fn from(outpoint: TransactionOutpointWrapper) -> Self {
        Self::new(outpoint.transaction_id, outpoint.index)
    }
}

impl From<UtxoEntryWrapper> for UtxoEntry {
    fn from(entry: UtxoEntryWrapper) -> Self {
        Self {
            address: entry.address,
            outpoint: entry.outpoint.into(),
            amount: entry.amount,
            script_public_key: entry.script_public_key,
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
        }
    }
}

impl From<UtxoEntry> for UtxoEntryWrapper {
    fn from(entry: UtxoEntry) -> Self {
        Self {
            address: entry.address,
            outpoint: entry.outpoint.into(),
            amount: entry.amount,
            script_public_key: entry.script_public_key,
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_coinbase,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsTransferRequest {
    pub source_account_id: AccountId,
    pub destination_account_id: AccountId,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub transfer_amount_sompi: u64,
    pub fee_rate: Option<f64>,
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
    pub fee_rate: Option<f64>,
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
pub struct FeeRateEstimateBucket {
    feerate: f64,
    seconds: f64,
}

impl From<RpcFeerateBucket> for FeeRateEstimateBucket {
    fn from(bucket: RpcFeerateBucket) -> Self {
        Self { feerate: bucket.feerate, seconds: bucket.estimated_seconds }
    }
}

impl From<&RpcFeerateBucket> for FeeRateEstimateBucket {
    fn from(bucket: &RpcFeerateBucket) -> Self {
        Self { feerate: bucket.feerate, seconds: bucket.estimated_seconds }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRateEstimateRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRateEstimateResponse {
    pub priority: FeeRateEstimateBucket,
    pub normal: FeeRateEstimateBucket,
    pub low: FeeRateEstimateBucket,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRatePollerEnableRequest {
    pub interval_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRatePollerEnableResponse {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRatePollerDisableRequest {}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRatePollerDisableResponse {}

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

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCommitRevealManualRequest {
    pub account_id: AccountId,
    pub script_sig: Vec<u8>,
    pub start_destination: PaymentDestination,
    pub end_destination: PaymentDestination,
    pub wallet_secret: Secret,
    pub payment_secret: Option<Secret>,
    pub fee_rate: Option<f64>,
    pub reveal_fee_sompi: u64,
    pub payload: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCommitRevealManualResponse {
    pub transaction_ids: Vec<TransactionId>,
}

/// Specifies the type of an account address to be used in
/// commit reveal redeem script and also to spend reveal
/// operation to.
///
/// @category Wallet API
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "wasm32-sdk", wasm_bindgen)]
pub enum CommitRevealAddressKind {
    Receive,
    Change,
}

impl FromStr for CommitRevealAddressKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "receive" => Ok(CommitRevealAddressKind::Receive),
            "change" => Ok(CommitRevealAddressKind::Change),
            _ => Err(Error::custom(format!("Invalid address kind: {s}"))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCommitRevealRequest {
    pub account_id: AccountId,
    pub address_type: CommitRevealAddressKind,
    pub address_index: u32,
    pub script_sig: Vec<u8>,
    pub wallet_secret: Secret,
    pub commit_amount_sompi: u64,
    pub payment_secret: Option<Secret>,
    pub fee_rate: Option<f64>,
    pub reveal_fee_sompi: u64,
    pub payload: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsCommitRevealResponse {
    pub transaction_ids: Vec<TransactionId>,
}
