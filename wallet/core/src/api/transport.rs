//!
//! Server and Client transport wrappers that provide automatic
//! serialization and deserialization of Wallet API method
//! arguments and their return values.
//!
//! The serialization occurs using the underlying transport
//! which can be either Borsh or Serde JSON. At compile time,
//! the transport interface macro generates a unique `u64` id
//! (hash) for each API method based on the method name.
//! This id is then use to identify the method.
//!

use super::message::*;
use super::traits::WalletApi;
use crate::error::Error;
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::wallet::Wallet;
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_wallet_macros::{build_wallet_client_transport_interface, build_wallet_server_transport_interface};
use workflow_core::task::spawn;

/// Transport interface supporting Borsh serialization
#[async_trait]
pub trait BorshCodec: Send + Sync {
    async fn call(&self, op: u64, request: Vec<u8>) -> Result<Vec<u8>>;
}

/// Transport interface supporting Serde JSON serialization
#[async_trait]
pub trait SerdeCodec: Send + Sync {
    async fn call(&self, op: &str, request: &str) -> Result<String>;
}

/// Transport interface enum supporting either Borsh and Serde JSON serialization
#[derive(Clone)]
pub enum Codec {
    Borsh(Arc<dyn BorshCodec>),
    Serde(Arc<dyn SerdeCodec>),
}

/// [`WalletClient`] is a client-side transport interface declaring
/// API methods that can be invoked via WalletApi method calls.
/// [`WalletClient`] is a counter-part to [`WalletServer`].
pub struct WalletClient {
    pub codec: Codec,
}

impl WalletClient {
    pub fn new(codec: Codec) -> Self {
        Self { codec }
    }
}

use workflow_core::channel::{DuplexChannel, Receiver};
#[async_trait]
impl WalletApi for WalletClient {
    async fn register_notifications(self: Arc<Self>, _channel: Receiver<WalletNotification>) -> Result<u64> {
        todo!()
    }
    async fn unregister_notifications(self: Arc<Self>, _channel_id: u64) -> Result<()> {
        todo!()
    }

    build_wallet_client_transport_interface! {[
        Ping,
        GetStatus,
        Connect,
        Disconnect,
        ChangeNetworkId,
        RetainContext,
        Batch,
        Flush,
        WalletEnumerate,
        WalletCreate,
        WalletOpen,
        WalletClose,
        WalletReload,
        WalletRename,
        WalletChangeSecret,
        WalletExport,
        WalletImport,
        PrvKeyDataEnumerate,
        PrvKeyDataCreate,
        PrvKeyDataRemove,
        PrvKeyDataGet,
        AccountsRename,
        AccountsSelect,
        AccountsEnumerate,
        AccountsDiscovery,
        AccountsCreate,
        AccountsEnsureDefault,
        AccountsImport,
        AccountsActivate,
        AccountsDeactivate,
        AccountsGet,
        AccountsCreateNewAddress,
        AccountsSend,
        AccountsTransfer,
        AccountsEstimate,
        TransactionsDataGet,
        TransactionsReplaceNote,
        TransactionsReplaceMetadata,
        AddressBookEnumerate,
    ]}
}

// ----------------------------

#[async_trait]
pub trait EventHandler: Send + Sync {
    // pub trait EventHandler {
    // async fn handle_event(&self, event: &Box<Events>);
    async fn handle_event(&self, event: &Events);
}

/// [`WalletServer`] is a server-side transport interface that declares
/// API methods that can be invoked via Borsh or Serde messages containing
/// serializations created using the [`Transport`] interface. The [`WalletServer`]
/// is a counter-part to [`WalletClient`].
pub struct WalletServer {
    // pub wallet_api: Arc<dyn WalletApi>,
    pub wallet: Arc<Wallet>,
    pub event_handler: Arc<dyn EventHandler>,
    task_ctl: DuplexChannel,
}

impl WalletServer {
    // pub fn new(wallet_api: Arc<dyn WalletApi>, event_handler : Arc<dyn EventHandler>) -> Self {
    //     Self { wallet_api, event_handler }
    pub fn new(wallet: Arc<Wallet>, event_handler: Arc<dyn EventHandler>) -> Self {
        Self { wallet, event_handler, task_ctl: DuplexChannel::unbounded() }
    }

    pub fn wallet_api(&self) -> Arc<dyn WalletApi> {
        self.wallet.clone()
    }
}

impl WalletServer {
    build_wallet_server_transport_interface! {[
        Ping,
        GetStatus,
        Connect,
        Disconnect,
        ChangeNetworkId,
        RetainContext,
        Batch,
        Flush,
        WalletEnumerate,
        WalletCreate,
        WalletOpen,
        WalletClose,
        WalletReload,
        WalletRename,
        WalletChangeSecret,
        WalletExport,
        WalletImport,
        PrvKeyDataEnumerate,
        PrvKeyDataCreate,
        PrvKeyDataRemove,
        PrvKeyDataGet,
        AccountsRename,
        AccountsSelect,
        AccountsEnumerate,
        AccountsDiscovery,
        AccountsCreate,
        AccountsEnsureDefault,
        AccountsImport,
        AccountsActivate,
        AccountsDeactivate,
        AccountsGet,
        AccountsCreateNewAddress,
        AccountsSend,
        AccountsTransfer,
        AccountsEstimate,
        TransactionsDataGet,
        TransactionsReplaceNote,
        TransactionsReplaceMetadata,
        AddressBookEnumerate,
    ]}
}

impl WalletServer {
    pub fn start(self: &Arc<Self>) {
        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();
        let events = self.wallet.multiplexer().channel();

        let this = self.clone();
        spawn(async move {
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },

                    msg = events.receiver.recv().fuse() => {
                        match msg {
                            Ok(event) => {
                                this.event_handler.handle_event(&event).await;//.unwrap_or_else(|e| log_error!("Wallet::handle_event() error: {}", e));
                            },
                            Err(err) => {
                                log_error!("Wallet: error while receiving multiplexer message: {err}");
                                log_error!("Suspending Wallet processing...");

                                break;
                            }
                        }
                    },
                }
            }

            task_ctl_sender.send(()).await.unwrap();
        });
    }

    pub async fn stop_task(&self) -> Result<()> {
        self.task_ctl.signal(()).await.expect("Wallet::stop_task() `signal` error");
        Ok(())
    }
}
