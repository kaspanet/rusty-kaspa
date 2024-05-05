//!
//! API trait for interfacing with the Kaspa wallet subsystem.
//!
//! The wallet API is a high-level API that allows applications to perform
//! wallet operations such as creating a wallet, opening a wallet, creating
//! accounts, sending funds etc. The wallet API is an asynchronous trait that
//! is implemented by the [`Wallet`] struct.
//!

use crate::api::message::*;
use crate::imports::*;
use crate::storage::{PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, WalletDescriptor};
use crate::tx::GeneratorSummary;
use workflow_core::channel::Receiver;

///
///  API trait for interfacing with the Kaspa wallet subsystem.
///
#[async_trait]
pub trait WalletApi: Send + Sync + AnySync {
    async fn register_notifications(self: Arc<Self>, channel: Receiver<WalletNotification>) -> Result<u64>;
    async fn unregister_notifications(self: Arc<Self>, channel_id: u64) -> Result<()>;

    async fn retain_context(self: Arc<Self>, name: &str, data: Option<Vec<u8>>) -> Result<()> {
        self.retain_context_call(RetainContextRequest { name: name.to_string(), data }).await?;
        Ok(())
    }

    async fn retain_context_call(self: Arc<Self>, request: RetainContextRequest) -> Result<RetainContextResponse>;

    /// Wrapper around [`get_status_call()`](Self::get_status_call).
    async fn get_status(self: Arc<Self>, name: Option<&str>) -> Result<GetStatusResponse> {
        Ok(self.get_status_call(GetStatusRequest { name: name.map(String::from) }).await?)
    }

    /// Returns the current wallet state comprised of the following:
    /// - `is_connected` - whether the wallet is connected to the node
    /// - `network_id` - the network id of the node the wallet is connected to
    /// - `is_synced` - the sync status of the node the wallet is connected to
    /// - `is_open` - whether a wallet is currently open
    /// - `url` - the wRPC url of the node the wallet is connected to
    /// - `is_wrpc_client` - whether the wallet is connected to a node via wRPC
    async fn get_status_call(self: Arc<Self>, request: GetStatusRequest) -> Result<GetStatusResponse>;

    async fn connect(self: Arc<Self>, url: Option<String>, network_id: NetworkId) -> Result<()> {
        self.connect_call(ConnectRequest { url, network_id }).await?;
        Ok(())
    }

    /// Request the wallet RPC subsystem to connect to a node with a given configuration
    /// comprised of the `url` and a `network_id`.
    async fn connect_call(self: Arc<Self>, request: ConnectRequest) -> Result<ConnectResponse>;

    async fn disconnect(self: Arc<Self>) -> Result<()> {
        self.disconnect_call(DisconnectRequest {}).await?;
        Ok(())
    }
    /// Disconnect the wallet RPC subsystem from the node.
    async fn disconnect_call(self: Arc<Self>, request: DisconnectRequest) -> Result<DisconnectResponse>;

    /// Wrapper around [`change_network_id_call()`](Self::change_network_id_call).
    async fn change_network_id(self: Arc<Self>, network_id: NetworkId) -> Result<()> {
        self.change_network_id_call(ChangeNetworkIdRequest { network_id }).await?;
        Ok(())
    }

    /// Change the current network id of the wallet.
    async fn change_network_id_call(self: Arc<Self>, request: ChangeNetworkIdRequest) -> Result<ChangeNetworkIdResponse>;

    // ---

    /// Wrapper around `ping_call()`.
    async fn ping(self: Arc<Self>, message: Option<String>) -> Result<Option<String>> {
        Ok(self.ping_call(PingRequest { message }).await?.message)
    }
    /// Ping the wallet service. Accepts an optional `u64` value that is returned in the response.
    async fn ping_call(self: Arc<Self>, request: PingRequest) -> Result<PingResponse>;

    async fn batch(self: Arc<Self>) -> Result<()> {
        self.batch_call(BatchRequest {}).await?;
        Ok(())
    }
    /// Initiates the wallet storage batch mode. Must be followed by the [`flush_call()`](Self::flush_call)
    /// after the desired wallet operations have been executed.
    ///
    /// Batch mode allows user to perform multiple wallet operations without storing the
    /// wallet data into the storage subsystem (disk, localstorage etc). This is helpful
    /// in web browsers as each time the wallet is stored, it needs to be encrypted,
    /// which can be costly in low-performance environments such as web browsers.
    ///
    async fn batch_call(self: Arc<Self>, request: BatchRequest) -> Result<BatchResponse>;

    async fn flush(self: Arc<Self>, wallet_secret: Secret) -> Result<()> {
        self.flush_call(FlushRequest { wallet_secret }).await?;
        Ok(())
    }
    /// Saves the wallet data into the storage subsystem (disk, localstorage etc) if
    /// the wallet is in the batch mode and it's data has been marked as dirty.
    async fn flush_call(self: Arc<Self>, request: FlushRequest) -> Result<FlushResponse>;

    /// Wrapper around [`wallet_enumerate_call()`](Self::wallet_enumerate_call).
    async fn wallet_enumerate(self: Arc<Self>) -> Result<Vec<WalletDescriptor>> {
        Ok(self.wallet_enumerate_call(WalletEnumerateRequest {}).await?.wallet_descriptors)
    }

    /// Enumerates all wallets available in the storage. Returns `Vec<WalletDescriptor>`
    /// that can be subsequently used to perform wallet operations such as open the wallet.
    /// See [`wallet_enumerate()`](Self::wallet_enumerate) for a convenience wrapper
    /// around this call.
    async fn wallet_enumerate_call(self: Arc<Self>, request: WalletEnumerateRequest) -> Result<WalletEnumerateResponse>;

    /// Wrapper around [`wallet_create_call()`](Self::wallet_create_call)
    async fn wallet_create(self: Arc<Self>, wallet_secret: Secret, wallet_args: WalletCreateArgs) -> Result<WalletCreateResponse> {
        self.wallet_create_call(WalletCreateRequest { wallet_secret, wallet_args }).await
    }

    /// Creates a new wallet. Returns [`WalletCreateResponse`] that contains `wallet_descriptor`
    /// that can be used to subsequently open the wallet. After the wallet is created, it
    /// is considered to be in an `open` state.
    async fn wallet_create_call(self: Arc<Self>, request: WalletCreateRequest) -> Result<WalletCreateResponse>;

    /// Wrapper around [`wallet_open_call()`](Self::wallet_open_call)
    async fn wallet_open(
        self: Arc<Self>,
        wallet_secret: Secret,
        filename: Option<String>,
        account_descriptors: bool,
        legacy_accounts: bool,
    ) -> Result<Option<Vec<AccountDescriptor>>> {
        Ok(self
            .wallet_open_call(WalletOpenRequest {
                wallet_secret,
                filename,
                account_descriptors,
                legacy_accounts: legacy_accounts.then_some(true),
            })
            .await?
            .account_descriptors)
    }

    /// Opens a wallet. A wallet is opened by it's `filename`, which is available
    /// as a part of the `WalletDescriptor` struct returned during the `wallet_enumerate_call()` call.
    /// If the `filename` is `None`, the wallet opens the default wallet named `kaspa`.
    ///
    /// If `account_descriptors` is true, this call will return `Some(Vec<AccountDescriptor>)`
    /// for all accounts in the wallet.
    ///
    /// If `legacy_accounts` is true, the wallet will enable legacy account compatibility mode
    /// allowing the wallet to operate on legacy accounts. Legacy accounts were created by
    /// applications such as KDX and kaspanet.io web wallet using a deprecated derivation path
    /// and are considered deprecated. Legacy accounts should not be used in 3rd-party applications.
    ///
    /// See [`wallet_open`](Self::wallet_open) for a convenience wrapper around this call.
    async fn wallet_open_call(self: Arc<Self>, request: WalletOpenRequest) -> Result<WalletOpenResponse>;

    /// Wrapper around [`wallet_close_call()`](Self::wallet_close_call)
    async fn wallet_close(self: Arc<Self>) -> Result<()> {
        self.wallet_close_call(WalletCloseRequest {}).await?;
        Ok(())
    }
    /// Close the currently open wallet
    async fn wallet_close_call(self: Arc<Self>, request: WalletCloseRequest) -> Result<WalletCloseResponse>;

    /// Wrapper around [`wallet_reload_call()`](Self::wallet_reload_call)
    async fn wallet_reload(self: Arc<Self>, reactivate: bool) -> Result<()> {
        self.wallet_reload_call(WalletReloadRequest { reactivate }).await?;
        Ok(())
    }

    /// Reload the currently open wallet. This call will re-read the wallet data from the
    /// storage subsystem (disk, localstorage etc) and optionally re-activate all accounts that were
    /// active before the reload.
    async fn wallet_reload_call(self: Arc<Self>, request: WalletReloadRequest) -> Result<WalletReloadResponse>;

    /// Wrapper around [`wallet_rename_call()`](Self::wallet_rename_call)
    async fn wallet_rename(self: Arc<Self>, title: Option<&str>, filename: Option<&str>, wallet_secret: Secret) -> Result<()> {
        self.wallet_rename_call(WalletRenameRequest {
            title: title.map(String::from),
            filename: filename.map(String::from),
            wallet_secret,
        })
        .await?;
        Ok(())
    }
    /// Change the wallet title or rename the file in which the wallet is stored.
    /// This call will produce an error if the destination filename already exists.
    /// See [`wallet_rename`](Self::wallet_rename) for a convenience wrapper around
    /// this call.
    async fn wallet_rename_call(self: Arc<Self>, request: WalletRenameRequest) -> Result<WalletRenameResponse>;

    /// Return a JSON string that contains raw wallet data. This is available only
    /// in the default wallet storage backend and may not be available if the wallet
    /// subsystem uses a custom storage backend.
    async fn wallet_export_call(self: Arc<Self>, request: WalletExportRequest) -> Result<WalletExportResponse>;

    /// Import the raw wallet data from a JSON string. This is available only
    /// in the default wallet storage backend and may not be available if the wallet
    /// subsystem uses a custom storage backend.
    async fn wallet_import_call(self: Arc<Self>, request: WalletImportRequest) -> Result<WalletImportResponse>;

    /// Wrapper around [`wallet_change_secret_call()`](Self::wallet_change_secret_call)
    async fn wallet_change_secret(self: Arc<Self>, old_wallet_secret: Secret, new_wallet_secret: Secret) -> Result<()> {
        let request = WalletChangeSecretRequest { old_wallet_secret, new_wallet_secret };
        self.wallet_change_secret_call(request).await?;
        Ok(())
    }

    /// Change the wallet secret. This call will re-encrypt the wallet data using the new secret.
    /// See [`wallet_change_secret`](Self::wallet_change_secret) for a convenience wrapper around
    /// this call.
    async fn wallet_change_secret_call(self: Arc<Self>, request: WalletChangeSecretRequest) -> Result<WalletChangeSecretResponse>;

    /// Wrapper around [`prv_key_data_enumerate_call()`](Self::prv_key_data_enumerate_call)
    async fn prv_key_data_enumerate(self: Arc<Self>) -> Result<Vec<Arc<PrvKeyDataInfo>>> {
        Ok(self.prv_key_data_enumerate_call(PrvKeyDataEnumerateRequest {}).await?.prv_key_data_list)
    }

    /// Enumerate all private key data available in the wallet.
    /// The returned [`PrvKeyDataEnumerateResponse`] contains a list
    /// of [`PrvKeyDataInfo`] structs that acts as private key descriptors.
    async fn prv_key_data_enumerate_call(self: Arc<Self>, request: PrvKeyDataEnumerateRequest) -> Result<PrvKeyDataEnumerateResponse>;

    /// Wrapper around [`prv_key_data_create_call()`](Self::prv_key_data_create_call)
    async fn prv_key_data_create(
        self: Arc<Self>,
        wallet_secret: Secret,
        prv_key_data_args: PrvKeyDataCreateArgs,
    ) -> Result<PrvKeyDataId> {
        let request = PrvKeyDataCreateRequest { wallet_secret, prv_key_data_args };
        Ok(self.prv_key_data_create_call(request).await?.prv_key_data_id)
    }
    /// Create a new private key data. This call receives a user-supplied bip39 mnemonic as well as
    /// an optional bip39 passphrase (payment secret). Please note that a mnemonic that contains
    /// bip39 passphrase is also encrypted at runtime using the same passphrase. This is specific
    /// to this wallet implementation. To gain access to such mnemonic, the user must supply the
    /// bip39 passphrase.
    ///
    /// See [`prv_key_data_create`](Self::prv_key_data_create) for a convenience wrapper around
    /// this call.
    async fn prv_key_data_create_call(self: Arc<Self>, request: PrvKeyDataCreateRequest) -> Result<PrvKeyDataCreateResponse>;

    /// Not implemented
    async fn prv_key_data_remove_call(self: Arc<Self>, request: PrvKeyDataRemoveRequest) -> Result<PrvKeyDataRemoveResponse>;

    /// Wrapper around [`prv_key_data_get_call()`](Self::prv_key_data_get_call)
    async fn prv_key_data_get(self: Arc<Self>, prv_key_data_id: PrvKeyDataId, wallet_secret: Secret) -> Result<PrvKeyData> {
        Ok(self
            .prv_key_data_get_call(PrvKeyDataGetRequest { prv_key_data_id, wallet_secret })
            .await?
            .prv_key_data
            .ok_or(Error::PrivateKeyNotFound(prv_key_data_id))?)
    }
    /// Obtain a private key data using [`PrvKeyDataId`].
    async fn prv_key_data_get_call(self: Arc<Self>, request: PrvKeyDataGetRequest) -> Result<PrvKeyDataGetResponse>;

    /// Wrapper around [`accounts_rename_call()`](Self::accounts_rename_call)
    async fn accounts_rename(self: Arc<Self>, account_id: AccountId, name: Option<String>, wallet_secret: Secret) -> Result<()> {
        self.accounts_rename_call(AccountsRenameRequest { account_id, name, wallet_secret }).await?;
        Ok(())
    }
    /// Change the account title.
    ///
    /// See [`accounts_rename`](Self::accounts_rename) for a convenience wrapper
    /// around this call.
    async fn accounts_rename_call(self: Arc<Self>, request: AccountsRenameRequest) -> Result<AccountsRenameResponse>;

    async fn accounts_select(self: Arc<Self>, account_id: Option<AccountId>) -> Result<()> {
        self.accounts_select_call(AccountsSelectRequest { account_id }).await?;
        Ok(())
    }

    /// Select an account. This call will set the currently *selected* account to the
    /// account specified by the `account_id`. The selected account is tracked within
    /// the wallet and can be obtained via get_status() API call.
    async fn accounts_select_call(self: Arc<Self>, request: AccountsSelectRequest) -> Result<AccountsSelectResponse>;

    /// Wrapper around [`accounts_activate_call()`](Self::accounts_activate_call)
    async fn accounts_activate(self: Arc<Self>, account_ids: Option<Vec<AccountId>>) -> Result<AccountsActivateResponse> {
        self.accounts_activate_call(AccountsActivateRequest { account_ids }).await
    }
    /// Activate a specific set of accounts.
    /// An account can be in 2 states - active and inactive. When an account
    /// is activated, it performs a discovery of UTXO entries related to its
    /// addresses, registers for appropriate notifications and starts tracking
    /// its state. As long as an account is active and the wallet is connected
    /// to the node, the account will give a consistent view of its state.
    /// Deactivating an account will cause it to unregister from notifications
    /// and stop tracking its state.
    ///
    async fn accounts_activate_call(self: Arc<Self>, request: AccountsActivateRequest) -> Result<AccountsActivateResponse>;

    /// Wrapper around [`accounts_deactivate_call()`](Self::accounts_deactivate_call)
    async fn accounts_deactivate(self: Arc<Self>, account_ids: Option<Vec<AccountId>>) -> Result<AccountsDeactivateResponse> {
        self.accounts_deactivate_call(AccountsDeactivateRequest { account_ids }).await
    }

    /// Deactivate a specific set of accounts. If `account_ids` in [`AccountsDeactivateRequest`]
    /// is `None`, all currently active accounts will be deactivated.
    async fn accounts_deactivate_call(self: Arc<Self>, request: AccountsDeactivateRequest) -> Result<AccountsDeactivateResponse>;

    /// Wrapper around [`accounts_enumerate_call()`](Self::accounts_enumerate_call)
    async fn accounts_enumerate(self: Arc<Self>) -> Result<Vec<AccountDescriptor>> {
        Ok(self.accounts_enumerate_call(AccountsEnumerateRequest {}).await?.account_descriptors)
    }
    /// Returns a list of [`AccountDescriptor`] structs for all accounts stored in the wallet.
    async fn accounts_enumerate_call(self: Arc<Self>, request: AccountsEnumerateRequest) -> Result<AccountsEnumerateResponse>;

    /// Performs a bip44 account discovery by scanning the account address space.
    /// Returns the last sequential bip44 index of an account that contains a balance.
    /// The discovery is performed by scanning `account_scan_extent` accounts where
    /// each account is scanned for `address_scan_extent` addresses. If a UTXO is found
    /// during the scan, ths account index and all account indexes preceding it are
    /// considered as viable.
    async fn accounts_discovery_call(self: Arc<Self>, request: AccountsDiscoveryRequest) -> Result<AccountsDiscoveryResponse>;

    /// Wrapper around [`accounts_create_call()`](Self::accounts_create_call)
    async fn accounts_create(
        self: Arc<Self>,
        wallet_secret: Secret,
        account_create_args: AccountCreateArgs,
    ) -> Result<AccountDescriptor> {
        Ok(self.accounts_create_call(AccountsCreateRequest { wallet_secret, account_create_args }).await?.account_descriptor)
    }
    /// Create a new account based on the [`AccountCreateArgs`] enum.
    /// Returns an [`AccountDescriptor`] for the newly created account.
    ///
    /// See [`accounts_create`](Self::accounts_create) for a convenience wrapper
    /// around this call.
    async fn accounts_create_call(self: Arc<Self>, request: AccountsCreateRequest) -> Result<AccountsCreateResponse>;

    /// Wrapper around [`accounts_ensure_default_call()`](Self::accounts_ensure_default_call)
    async fn accounts_ensure_default(
        self: Arc<Self>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        account_kind: AccountKind,
        mnemonic_phrase: Option<Secret>,
    ) -> Result<AccountDescriptor> {
        let request = AccountsEnsureDefaultRequest { wallet_secret, payment_secret, account_kind, mnemonic_phrase };
        Ok(self.accounts_ensure_default_call(request).await?.account_descriptor)
    }

    /// Ensure that a default account exists. If the default account does not exist,
    /// this call will create a new private key and an associated account and return
    /// an [`AccountDescriptor`] for it. A custom mnemonic phrase can be supplied
    /// for the private key. This function currently supports only BIP32 accounts.
    /// If a `payment_secret` is supplied, the mnemonic phrase will be created
    /// with a BIP39 passphrase.
    async fn accounts_ensure_default_call(
        self: Arc<Self>,
        request: AccountsEnsureDefaultRequest,
    ) -> Result<AccountsEnsureDefaultResponse>;

    // TODO
    async fn accounts_import_call(self: Arc<Self>, request: AccountsImportRequest) -> Result<AccountsImportResponse>;

    /// Get an [`AccountDescriptor`] for a specific account id.
    async fn accounts_get_call(self: Arc<Self>, request: AccountsGetRequest) -> Result<AccountsGetResponse>;

    /// Wrapper around [`accounts_create_new_address`](Self::accounts_create_new_address)
    async fn accounts_create_new_address(
        self: Arc<Self>,
        account_id: AccountId,
        kind: NewAddressKind,
    ) -> Result<AccountsCreateNewAddressResponse> {
        self.accounts_create_new_address_call(AccountsCreateNewAddressRequest { account_id, kind }).await
    }

    /// Creates a new address for a specified account id. This call is applicable
    /// only to derivation-capable accounts (bip32 and legacy accounts). Returns
    /// a [`AccountsCreateNewAddressResponse`] that contains a newly generated address.
    async fn accounts_create_new_address_call(
        self: Arc<Self>,
        request: AccountsCreateNewAddressRequest,
    ) -> Result<AccountsCreateNewAddressResponse>;

    /// Wrapper around [`Self::accounts_send_call()`](Self::accounts_send_call)
    async fn accounts_send(self: Arc<Self>, request: AccountsSendRequest) -> Result<GeneratorSummary> {
        Ok(self.accounts_send_call(request).await?.generator_summary)
    }
    /// Send funds from an account to one or more external addresses. Returns
    /// an [`AccountsSendResponse`] struct that contains a [`GeneratorSummary`] as
    /// well `transaction_ids` containing a list of submitted transaction ids.
    async fn accounts_send_call(self: Arc<Self>, request: AccountsSendRequest) -> Result<AccountsSendResponse>;

    /// Transfer funds to another account. Returns an [`AccountsTransferResponse`]
    /// struct that contains a [`GeneratorSummary`] as well `transaction_ids`
    /// containing a list of submitted transaction ids. Unlike funds sent to an
    /// external address, funds transferred between wallet accounts are
    /// available immediately upon transaction acceptance.
    async fn accounts_transfer_call(self: Arc<Self>, request: AccountsTransferRequest) -> Result<AccountsTransferResponse>;

    /// Performs a transaction estimate, returning [`AccountsEstimateResponse`]
    /// that contains [`GeneratorSummary`]. This call will estimate the total
    /// amount of fees that will be required by the transaction as well as
    /// the number of UTXOs that will be consumed by the transaction. If this
    /// call is invoked while the previous instance of this call is already
    /// running for the same account, the previous call will be aborted returning
    /// an error.
    async fn accounts_estimate_call(self: Arc<Self>, request: AccountsEstimateRequest) -> Result<AccountsEstimateResponse>;

    /// Get a range of transaction records for a specific account id.
    async fn transactions_data_get_range(
        self: Arc<Self>,
        account_id: AccountId,
        network_id: NetworkId,
        range: std::ops::Range<u64>,
    ) -> Result<TransactionsDataGetResponse> {
        self.transactions_data_get_call(TransactionsDataGetRequest::with_range(account_id, network_id, range)).await
    }

    async fn transactions_data_get_call(self: Arc<Self>, request: TransactionsDataGetRequest) -> Result<TransactionsDataGetResponse>;
    // async fn transaction_get_call(self: Arc<Self>, request: TransactionGetRequest) -> Result<TransactionGetResponse>;

    /// Replaces the note of a transaction with a new note. Note is meant
    /// to explicitly store a user-supplied string. The note is treated
    /// as a raw string without any assumptions about the note format.
    ///
    /// Supply [`Option::None`] in the `note` field to remove the note.
    async fn transactions_replace_note_call(
        self: Arc<Self>,
        request: TransactionsReplaceNoteRequest,
    ) -> Result<TransactionsReplaceNoteResponse>;

    /// Replaces the metadata of a transaction with a new metadata.
    /// Metadata is meant to store an application-specific data.
    /// If used, the application and encode custom JSON data into the
    /// metadata string. The metadata is treated as a raw string
    /// without any assumptions about the metadata format.
    ///
    /// Supply [`Option::None`] in the `metadata` field to
    /// remove the metadata.
    async fn transactions_replace_metadata_call(
        self: Arc<Self>,
        request: TransactionsReplaceMetadataRequest,
    ) -> Result<TransactionsReplaceMetadataResponse>;

    async fn address_book_enumerate_call(
        self: Arc<Self>,
        request: AddressBookEnumerateRequest,
    ) -> Result<AddressBookEnumerateResponse>;
}

/// alias for `Arc<dyn WalletApi + Send + Sync + 'static>`
pub type DynWalletApi = Arc<dyn WalletApi + Send + Sync + 'static>;

downcast_sync!(dyn WalletApi);
