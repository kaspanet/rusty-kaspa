use std::sync::Arc;

use super::message::*;
use super::traits::WalletApi;
use crate::error::Error;
use crate::result::Result;
use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_wallet_macros::{build_wallet_client_transport_interface, build_wallet_server_transport_interface};
// use serde::de::DeserializeOwned;
// use serde::{Deserialize, Serialize};

#[async_trait]
pub trait Transport {
    async fn call(&self, op: u64, request: &[u8]) -> Result<Vec<u8>>;
}

// - TODO - WALLET SERVER

pub struct WalletServer {
    pub wallet_api: Arc<dyn WalletApi>,
}

#[async_trait]
impl Transport for WalletServer {
    async fn call(&self, op: u64, request: &[u8]) -> Result<Vec<u8>> {
        build_wallet_server_transport_interface! {[
            WalletEnumerate,
            WalletCreate,
            WalletOpen,
            WalletClose,
            PrvKeyDataCreate,
            PrvKeyDataRemove,
            PrvKeyDataGet,
            AccountEnumerate,
            AccountCreate,
            AccountImport,
            AccountGet,
            AccountCreateNewAddress,
            AccountSend,
            AccountEstimate,
            TransactionDataGet,
            AddressBookEnumerate,
        ]}
    }
}

pub struct WalletClient {
    pub client_sender: Arc<dyn Transport + Send + Sync>,
}

impl WalletClient {
    pub fn new(client_sender: Arc<dyn Transport + Send + Sync>) -> Self {
        Self { client_sender }
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

    async fn connection_status_call(self: Arc<Self>, _request: ConnectionStatusRequest) -> Result<ConnectionStatusResponse> {
        todo!()
    }

    // -------------------------------------------------------------------------------------

    async fn connection_settings_get_call(
        self: Arc<Self>,
        _request: ConnectionSettingsGetRequest,
    ) -> Result<ConnectionSettingsGetResponse> {
        todo!()
    }

    async fn connection_settings_set_call(
        self: Arc<Self>,
        _request: ConnectionSettingsSetRequest,
    ) -> Result<ConnectionSettingsSetResponse> {
        todo!()
    }

    // -------------------------------------------------------------------------------------

    build_wallet_client_transport_interface! {[
        WalletEnumerate,
        WalletCreate,
        WalletOpen,
        WalletClose,
        PrvKeyDataCreate,
        PrvKeyDataRemove,
        PrvKeyDataGet,
        AccountEnumerate,
        AccountCreate,
        AccountImport,
        AccountGet,
        AccountCreateNewAddress,
        AccountSend,
        AccountEstimate,
        TransactionDataGet,
        AddressBookEnumerate,
    ]}
}
