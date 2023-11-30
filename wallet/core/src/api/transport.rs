use std::sync::Arc;

use super::message::*;
use super::traits::WalletApi;
use crate::error::Error;
use crate::result::Result;
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_wallet_macros::{build_wallet_client_transport_interface, build_wallet_server_transport_interface};

#[async_trait]
pub trait BorshTransport: Send + Sync {
    async fn call(&self, op: u64, request: Vec<u8>) -> Result<Vec<u8>>;
}

#[async_trait]
pub trait SerdeTransport: Send + Sync {
    async fn call(&self, op: &str, request: &str) -> Result<String>;
}

#[derive(Clone)]
pub enum Transport {
    Borsh(Arc<dyn BorshTransport>),
    Serde(Arc<dyn SerdeTransport>),
}

pub struct WalletServer {
    pub wallet_api: Arc<dyn WalletApi>,
}

impl WalletServer {
    pub fn new(wallet_api: Arc<dyn WalletApi>) -> Self {
        Self { wallet_api }
    }

    pub fn wallet_api(&self) -> &Arc<dyn WalletApi> {
        &self.wallet_api
    }
}

impl WalletServer {
    build_wallet_server_transport_interface! {[
        Ping,
        GetStatus,
        Connect,
        Disconnect,
        WalletEnumerate,
        WalletCreate,
        WalletOpen,
        WalletClose,
        WalletRename,
        PrvKeyDataEnumerate,
        PrvKeyDataCreate,
        PrvKeyDataRemove,
        PrvKeyDataGet,
        AccountsRename,
        AccountsEnumerate,
        AccountsCreate,
        AccountsImport,
        AccountsActivate,
        AccountsGet,
        AccountsCreateNewAddress,
        AccountsSend,
        AccountsTransfer,
        AccountsEstimate,
        TransactionDataGet,
        AddressBookEnumerate,
    ]}
}

pub struct WalletClient {
    pub transport: Transport,
}

impl WalletClient {
    pub fn new(transport: Transport) -> Self {
        Self { transport }
    }
}

use workflow_core::channel::Receiver;
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
        WalletEnumerate,
        WalletCreate,
        WalletOpen,
        WalletClose,
        WalletRename,
        PrvKeyDataEnumerate,
        PrvKeyDataCreate,
        PrvKeyDataRemove,
        PrvKeyDataGet,
        AccountsRename,
        AccountsEnumerate,
        AccountsCreate,
        AccountsImport,
        AccountsActivate,
        AccountsGet,
        AccountsCreateNewAddress,
        AccountsSend,
        AccountsTransfer,
        AccountsEstimate,
        TransactionDataGet,
        AddressBookEnumerate,
    ]}
}
