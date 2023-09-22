use crate::imports::*;
// use crate::runtime::Wallet;
use crate::result::Result;
use crate::runtime::api::message::*;
use crate::runtime::{AccountCreateArgs, PrvKeyDataCreateArgs, WalletCreateArgs};
use crate::storage::WalletDescriptor;
use workflow_core::channel::Receiver;
// use ;

// - TODO
//

// Id

// fn generate_task_id() -> TaskId {
//     TaskId::generate()
// }

#[async_trait]
pub trait WalletApi: Send + Sync + 'static {
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
    async fn wallet_open_call(self: Arc<Self>, request: WalletOpenRequest) -> Result<WalletOpenResponse>;
    async fn wallet_close_call(self: Arc<Self>, request: WalletCloseRequest) -> Result<WalletCloseResponse>;
    async fn prv_key_data_create_call(self: Arc<Self>, request: PrvKeyDataCreateRequest) -> Result<PrvKeyDataCreateResponse>;
    async fn prv_key_data_remove_call(self: Arc<Self>, request: PrvKeyDataRemoveRequest) -> Result<PrvKeyDataRemoveResponse>;
    async fn prv_key_data_get_call(self: Arc<Self>, request: PrvKeyDataGetRequest) -> Result<PrvKeyDataGetResponse>;
    async fn account_enumerate_call(self: Arc<Self>, request: AccountEnumerateRequest) -> Result<AccountEnumerateResponse>;
    async fn account_create_call(self: Arc<Self>, request: AccountCreateRequest) -> Result<AccountCreateResponse>;
    async fn account_import_call(self: Arc<Self>, request: AccountImportRequest) -> Result<AccountImportResponse>;
    async fn account_get_call(self: Arc<Self>, request: AccountGetRequest) -> Result<AccountGetResponse>;
    async fn account_create_new_address_call(
        self: Arc<Self>,
        request: AccountCreateNewAddressRequest,
    ) -> Result<AccountCreateNewAddressResponse>;
    async fn account_send_call(self: Arc<Self>, request: AccountSendRequest) -> Result<AccountSendResponse>;
    async fn account_estimate_call(self: Arc<Self>, request: AccountEstimateRequest) -> Result<AccountEstimateResponse>;
    async fn transaction_data_get_call(self: Arc<Self>, request: TransactionDataGetRequest) -> Result<TransactionDataGetResponse>;
    // async fn transaction_get_call(self: Arc<Self>, request: TransactionGetRequest) -> Result<TransactionGetResponse>;
    async fn address_book_enumerate_call(
        self: Arc<Self>,
        request: AddressBookEnumerateRequest,
    ) -> Result<AddressBookEnumerateResponse>;
}

pub type DynWalletApi = Arc<dyn WalletApi + Send + Sync + 'static>;
