use crate::api::message::*;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::wallet::AccountCreateArgs;
use crate::runtime::{AccountDescriptor, PrvKeyDataCreateArgs, WalletCreateArgs};
use crate::secret::Secret;
use crate::storage::{PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, WalletDescriptor};
use crate::tx::GeneratorSummary;
use runtime::AccountId;
use workflow_core::channel::Receiver;

#[async_trait]
pub trait WalletApi: Send + Sync + AnySync {
    async fn register_notifications(self: Arc<Self>, channel: Receiver<WalletNotification>) -> Result<u64>;
    async fn unregister_notifications(self: Arc<Self>, channel_id: u64) -> Result<()>;

    async fn get_status(self: Arc<Self>) -> Result<GetStatusResponse> {
        Ok(self.get_status_call(GetStatusRequest {}).await?)
    }
    async fn get_status_call(self: Arc<Self>, request: GetStatusRequest) -> Result<GetStatusResponse>;
    async fn connect_call(self: Arc<Self>, request: ConnectRequest) -> Result<ConnectResponse>;
    async fn disconnect_call(self: Arc<Self>, request: DisconnectRequest) -> Result<DisconnectResponse>;

    async fn ping(self: Arc<Self>, payload: Option<String>) -> Result<Option<String>> {
        Ok(self.ping_call(PingRequest { payload }).await?.payload)
    }
    async fn ping_call(self: Arc<Self>, request: PingRequest) -> Result<PingResponse>;

    async fn batch_call(self: Arc<Self>, request: BatchRequest) -> Result<BatchResponse>;
    async fn flush_call(self: Arc<Self>, request: FlushRequest) -> Result<FlushResponse>;

    async fn wallet_enumerate(self: Arc<Self>) -> Result<Vec<WalletDescriptor>> {
        Ok(self.wallet_enumerate_call(WalletEnumerateRequest {}).await?.wallet_list)
    }
    async fn wallet_enumerate_call(self: Arc<Self>, request: WalletEnumerateRequest) -> Result<WalletEnumerateResponse>;

    async fn wallet_create(self: Arc<Self>, wallet_secret: Secret, wallet_args: WalletCreateArgs) -> Result<WalletCreateResponse> {
        self.wallet_create_call(WalletCreateRequest { wallet_secret, wallet_args }).await
    }

    async fn wallet_create_call(self: Arc<Self>, request: WalletCreateRequest) -> Result<WalletCreateResponse>;

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

    async fn wallet_rename(self: Arc<Self>, title: Option<&str>, filename: Option<&str>, wallet_secret: Secret) -> Result<()> {
        self.wallet_rename_call(WalletRenameRequest {
            title: title.map(String::from),
            filename: filename.map(String::from),
            wallet_secret,
        })
        .await?;
        Ok(())
    }
    async fn wallet_rename_call(self: Arc<Self>, request: WalletRenameRequest) -> Result<WalletRenameResponse>;

    async fn wallet_export_call(self: Arc<Self>, request: WalletExportRequest) -> Result<WalletExportResponse>;
    async fn wallet_import_call(self: Arc<Self>, request: WalletImportRequest) -> Result<WalletImportResponse>;

    async fn wallet_change_secret(self: Arc<Self>, old_wallet_secret: Secret, new_wallet_secret: Secret) -> Result<()> {
        let request = WalletChangeSecretRequest { old_wallet_secret, new_wallet_secret };
        self.wallet_change_secret_call(request).await?;
        Ok(())
    }
    async fn wallet_change_secret_call(self: Arc<Self>, request: WalletChangeSecretRequest) -> Result<WalletChangeSecretResponse>;

    async fn prv_key_data_enumerate(self: Arc<Self>) -> Result<Vec<Arc<PrvKeyDataInfo>>> {
        Ok(self.prv_key_data_enumerate_call(PrvKeyDataEnumerateRequest {}).await?.prv_key_data_list)
    }

    async fn prv_key_data_enumerate_call(self: Arc<Self>, request: PrvKeyDataEnumerateRequest) -> Result<PrvKeyDataEnumerateResponse>;

    async fn prv_key_data_create(
        self: Arc<Self>,
        wallet_secret: Secret,
        prv_key_data_args: PrvKeyDataCreateArgs,
    ) -> Result<PrvKeyDataId> {
        let request = PrvKeyDataCreateRequest { wallet_secret, prv_key_data_args };
        Ok(self.prv_key_data_create_call(request).await?.prv_key_data_id)
    }
    async fn prv_key_data_create_call(self: Arc<Self>, request: PrvKeyDataCreateRequest) -> Result<PrvKeyDataCreateResponse>;

    async fn prv_key_data_remove_call(self: Arc<Self>, request: PrvKeyDataRemoveRequest) -> Result<PrvKeyDataRemoveResponse>;

    async fn prv_key_data_get(self: Arc<Self>, prv_key_data_id: PrvKeyDataId, wallet_secret: Secret) -> Result<PrvKeyData> {
        Ok(self
            .prv_key_data_get_call(PrvKeyDataGetRequest { prv_key_data_id, wallet_secret })
            .await?
            .prv_key_data
            .ok_or(Error::PrivateKeyNotFound(prv_key_data_id))?)
    }
    async fn prv_key_data_get_call(self: Arc<Self>, request: PrvKeyDataGetRequest) -> Result<PrvKeyDataGetResponse>;

    async fn accounts_rename(self: Arc<Self>, account_id: AccountId, name: Option<String>, wallet_secret: Secret) -> Result<()> {
        self.accounts_rename_call(AccountsRenameRequest { account_id, name, wallet_secret }).await?;
        Ok(())
    }
    async fn accounts_rename_call(self: Arc<Self>, request: AccountsRenameRequest) -> Result<AccountsRenameResponse>;

    async fn accounts_activate(self: Arc<Self>, account_ids: Option<Vec<runtime::AccountId>>) -> Result<AccountsActivateResponse> {
        self.accounts_activate_call(AccountsActivateRequest { account_ids }).await
    }
    async fn accounts_activate_call(self: Arc<Self>, request: AccountsActivateRequest) -> Result<AccountsActivateResponse>;

    async fn accounts_enumerate(self: Arc<Self>) -> Result<Vec<AccountDescriptor>> {
        Ok(self.accounts_enumerate_call(AccountsEnumerateRequest {}).await?.descriptor_list)
    }
    async fn accounts_enumerate_call(self: Arc<Self>, request: AccountsEnumerateRequest) -> Result<AccountsEnumerateResponse>;

    async fn accounts_discovery_call(self: Arc<Self>, request: AccountsDiscoveryRequest) -> Result<AccountsDiscoveryResponse>;

    async fn accounts_create(
        self: Arc<Self>,
        wallet_secret: Secret,
        account_create_args: AccountCreateArgs,
    ) -> Result<AccountDescriptor> {
        Ok(self.accounts_create_call(AccountsCreateRequest { wallet_secret, account_create_args }).await?.account_descriptor)
    }
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
