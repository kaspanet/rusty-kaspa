use crate::api::message::*;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{AccountCreateArgs, AccountDescriptor, PrvKeyDataCreateArgs, WalletCreateArgs};
use crate::secret::Secret;
use crate::storage::WalletDescriptor;
use crate::tx::GeneratorSummary;
use workflow_core::channel::Receiver;

#[async_trait]
pub trait WalletApi: Send + Sync + AnySync {
    async fn register_notifications(self: Arc<Self>, channel: Receiver<WalletNotification>) -> Result<u64>;
    async fn unregister_notifications(self: Arc<Self>, channel_id: u64) -> Result<()>;

    async fn connection_status_call(self: Arc<Self>, request: ConnectionStatusRequest) -> Result<ConnectionStatusResponse>;
    async fn connection_settings_get_call(
        self: Arc<Self>,
        request: ConnectionSettingsGetRequest,
    ) -> Result<ConnectionSettingsGetResponse>;
    async fn connection_settings_set_call(
        self: Arc<Self>,
        request: ConnectionSettingsSetRequest,
    ) -> Result<ConnectionSettingsSetResponse>;

    async fn wallet_enumerate(self: Arc<Self>) -> Result<Vec<WalletDescriptor>> {
        Ok(self.wallet_enumerate_call(WalletEnumerateRequest {}).await?.wallet_list)
    }
    async fn wallet_enumerate_call(self: Arc<Self>, request: WalletEnumerateRequest) -> Result<WalletEnumerateResponse>;

    async fn wallet_create(
        self: Arc<Self>,
        wallet_args: WalletCreateArgs,
        prv_key_data_args: PrvKeyDataCreateArgs,
        account_args: AccountCreateArgs,
    ) -> Result<WalletCreateResponse> {
        self.wallet_create_call(WalletCreateRequest { wallet_args, prv_key_data_args, account_args }).await
    }

    async fn wallet_create_call(self: Arc<Self>, request: WalletCreateRequest) -> Result<WalletCreateResponse>;

    async fn ping(self: Arc<Self>, v: u32) -> Result<u32> {
        Ok(self.ping_call(PingRequest { v }).await?.v)
    }
    async fn ping_call(self: Arc<Self>, request: PingRequest) -> Result<PingResponse>;

    async fn wallet_open(
        self: Arc<Self>,
        wallet_secret: Secret,
        wallet_name: Option<String>,
        account_descriptors: bool,
        legacy_accounts: bool,
    ) -> Result<Option<Vec<AccountDescriptor>>> {
        Ok(self
            .wallet_open_call(WalletOpenRequest {
                wallet_secret,
                wallet_name,
                account_descriptors,
                legacy_accounts: legacy_accounts.then_some(true),
            })
            .await?
            .account_descriptors)
    }

    async fn wallet_open_call(self: Arc<Self>, request: WalletOpenRequest) -> Result<WalletOpenResponse>;
    async fn wallet_close_call(self: Arc<Self>, request: WalletCloseRequest) -> Result<WalletCloseResponse>;

    async fn accounts_activate(self: Arc<Self>, account_ids: Option<Vec<runtime::AccountId>>) -> Result<AccountsActivateResponse> {
        self.accounts_activate_call(AccountsActivateRequest { account_ids }).await
    }
    async fn accounts_activate_call(self: Arc<Self>, request: AccountsActivateRequest) -> Result<AccountsActivateResponse>;

    async fn prv_key_data_enumerate_call(self: Arc<Self>, request: PrvKeyDataEnumerateRequest) -> Result<PrvKeyDataEnumerateResponse>;
    async fn prv_key_data_create_call(self: Arc<Self>, request: PrvKeyDataCreateRequest) -> Result<PrvKeyDataCreateResponse>;
    async fn prv_key_data_remove_call(self: Arc<Self>, request: PrvKeyDataRemoveRequest) -> Result<PrvKeyDataRemoveResponse>;
    async fn prv_key_data_get_call(self: Arc<Self>, request: PrvKeyDataGetRequest) -> Result<PrvKeyDataGetResponse>;

    async fn accounts_enumerate(self: Arc<Self>) -> Result<Vec<AccountDescriptor>> {
        Ok(self.accounts_enumerate_call(AccountsEnumerateRequest {}).await?.descriptor_list)
    }
    async fn accounts_enumerate_call(self: Arc<Self>, request: AccountsEnumerateRequest) -> Result<AccountsEnumerateResponse>;

    async fn accounts_create_call(self: Arc<Self>, request: AccountsCreateRequest) -> Result<AccountsCreateResponse>;
    async fn accounts_import_call(self: Arc<Self>, request: AccountsImportRequest) -> Result<AccountsImportResponse>;
    async fn accounts_get_call(self: Arc<Self>, request: AccountsGetRequest) -> Result<AccountsGetResponse>;

    async fn accounts_create_new_address(
        self: Arc<Self>,
        account_id: runtime::AccountId,
        kind: NewAddressKind,
    ) -> Result<AccountsCreateNewAddressResponse> {
        self.accounts_create_new_address_call(AccountsCreateNewAddressRequest { account_id, kind }).await
    }
    async fn accounts_create_new_address_call(
        self: Arc<Self>,
        request: AccountsCreateNewAddressRequest,
    ) -> Result<AccountsCreateNewAddressResponse>;

    async fn accounts_send(self: Arc<Self>, request: AccountsSendRequest) -> Result<GeneratorSummary> {
        Ok(self.accounts_send_call(request).await?.generator_summary)
    }
    async fn accounts_send_call(self: Arc<Self>, request: AccountsSendRequest) -> Result<AccountsSendResponse>;
    async fn accounts_transfer_call(self: Arc<Self>, request: AccountsTransferRequest) -> Result<AccountsTransferResponse>;

    // async fn account_estimate(self: Arc<Self>, request: AccountEstimateRequest) -> Result<AccountEstimateResponse> {

    //     Ok(self.account_estimate_call(request).await?)
    // }
    async fn accounts_estimate_call(self: Arc<Self>, request: AccountsEstimateRequest) -> Result<AccountsEstimateResponse>;

    async fn transaction_data_get_range(
        self: Arc<Self>,
        account_id: runtime::AccountId,
        network_id: NetworkId,
        range: std::ops::Range<u64>,
    ) -> Result<TransactionDataGetResponse> {
        self.transaction_data_get_call(TransactionDataGetRequest::with_range(account_id, network_id, range)).await
    }

    async fn transaction_data_get_call(self: Arc<Self>, request: TransactionDataGetRequest) -> Result<TransactionDataGetResponse>;
    // async fn transaction_get_call(self: Arc<Self>, request: TransactionGetRequest) -> Result<TransactionGetResponse>;
    async fn address_book_enumerate_call(
        self: Arc<Self>,
        request: AddressBookEnumerateRequest,
    ) -> Result<AddressBookEnumerateResponse>;
}

pub type DynWalletApi = Arc<dyn WalletApi + Send + Sync + 'static>;

downcast_sync!(dyn WalletApi);
