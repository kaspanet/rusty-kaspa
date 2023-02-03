use crate::protowire::{kaspad_request, kaspad_response, KaspadRequest, KaspadResponse};
use rpc_core::api::ops::RpcApiOps;

impl From<&kaspad_request::Payload> for RpcApiOps {
    fn from(item: &kaspad_request::Payload) -> Self {
        use kaspad_request::Payload;
        match item {
            Payload::SubmitBlockRequest(_) => RpcApiOps::SubmitBlock,
            Payload::GetBlockTemplateRequest(_) => RpcApiOps::GetBlockTemplate,
            Payload::GetCurrentNetworkRequest(_) => RpcApiOps::GetCurrentNetwork,
            Payload::GetBlockRequest(_) => RpcApiOps::GetBlock,
            Payload::GetBlocksRequest(_) => RpcApiOps::GetBlocks,
            Payload::GetInfoRequest(_) => RpcApiOps::GetInfo,

            Payload::ShutdownRequest(_) => RpcApiOps::Shutdown,
            Payload::GetPeerAddressesRequest(_) => RpcApiOps::GetPeerAddresses,
            Payload::GetSelectedTipHashRequest(_) => RpcApiOps::GetSelectedTipHash,
            Payload::GetMempoolEntryRequest(_) => RpcApiOps::GetMempoolEntry,
            Payload::GetMempoolEntriesRequest(_) => RpcApiOps::GetMempoolEntries,
            Payload::GetConnectedPeerInfoRequest(_) => RpcApiOps::GetConnectedPeerInfo,
            Payload::AddPeerRequest(_) => RpcApiOps::AddPeer,
            Payload::SubmitTransactionRequest(_) => RpcApiOps::SubmitTransaction,
            Payload::GetSubnetworkRequest(_) => RpcApiOps::GetSubnetwork,
            Payload::GetVirtualSelectedParentChainFromBlockRequest(_) => RpcApiOps::GetVirtualSelectedParentChainFromBlock,
            Payload::GetBlockCountRequest(_) => RpcApiOps::GetBlockCount,
            Payload::GetBlockDagInfoRequest(_) => RpcApiOps::GetBlockDagInfo,
            Payload::ResolveFinalityConflictRequest(_) => RpcApiOps::ResolveFinalityConflict,
            Payload::GetHeadersRequest(_) => RpcApiOps::GetHeaders,
            Payload::GetUtxosByAddressesRequest(_) => RpcApiOps::GetUtxosByAddresses,
            Payload::GetBalanceByAddressRequest(_) => RpcApiOps::GetBalanceByAddress,
            Payload::GetBalancesByAddressesRequest(_) => RpcApiOps::GetBalancesByAddresses,
            Payload::GetVirtualSelectedParentBlueScoreRequest(_) => RpcApiOps::GetVirtualSelectedParentBlueScore,
            Payload::BanRequest(_) => RpcApiOps::Ban,
            Payload::UnbanRequest(_) => RpcApiOps::Unban,
            Payload::EstimateNetworkHashesPerSecondRequest(_) => RpcApiOps::EstimateNetworkHashesPerSecond,
            Payload::GetMempoolEntriesByAddressesRequest(_) => RpcApiOps::GetMempoolEntriesByAddresses,
            Payload::GetCoinSupplyRequest(_) => RpcApiOps::GetCoinSupply,
            Payload::PingRequest(_) => RpcApiOps::Ping,
            Payload::GetProcessMetricsRequest(_) => RpcApiOps::GetProcessMetrics,

            // Subscription commands for starting/stopping notifications
            Payload::NotifyBlockAddedRequest(_) => RpcApiOps::NotifyBlockAdded,
            Payload::NotifyNewBlockTemplateRequest(_) => RpcApiOps::NotifyNewBlockTemplate,

            // ???
            Payload::NotifyFinalityConflictsRequest(_) => RpcApiOps::NotifyFinalityConflicts,
            Payload::NotifyUtxosChangedRequest(_) => RpcApiOps::NotifyUtxosChanged,
            Payload::NotifyVirtualSelectedParentBlueScoreChangedRequest(_) => RpcApiOps::NotifyVirtualSelectedParentBlueScoreChanged,
            Payload::NotifyPruningPointUtxoSetOverrideRequest(_) => RpcApiOps::NotifyPruningPointUtxoSetOverride,
            Payload::NotifyVirtualDaaScoreChangedRequest(_) => RpcApiOps::NotifyVirtualDaaScoreChanged,
            Payload::NotifyVirtualSelectedParentChainChangedRequest(_) => RpcApiOps::NotifyVirtualSelectedParentChainChanged,
        }
    }
}

impl From<&kaspad_response::Payload> for RpcApiOps {
    fn from(item: &kaspad_response::Payload) -> Self {
        use kaspad_response::Payload;
        match item {
            Payload::SubmitBlockResponse(_) => RpcApiOps::SubmitBlock,
            Payload::GetBlockTemplateResponse(_) => RpcApiOps::GetBlockTemplate,
            Payload::GetCurrentNetworkResponse(_) => RpcApiOps::GetCurrentNetwork,
            Payload::GetBlockResponse(_) => RpcApiOps::GetBlock,
            Payload::GetBlocksResponse(_) => RpcApiOps::GetBlocks,
            Payload::GetInfoResponse(_) => RpcApiOps::GetInfo,
            Payload::ShutdownResponse(_) => RpcApiOps::Shutdown,

            Payload::GetPeerAddressesResponse(_) => RpcApiOps::GetPeerAddresses,
            Payload::GetSelectedTipHashResponse(_) => RpcApiOps::GetSelectedTipHash,
            Payload::GetMempoolEntryResponse(_) => RpcApiOps::GetMempoolEntry,
            Payload::GetMempoolEntriesResponse(_) => RpcApiOps::GetMempoolEntries,
            Payload::GetConnectedPeerInfoResponse(_) => RpcApiOps::GetConnectedPeerInfo,
            Payload::AddPeerResponse(_) => RpcApiOps::AddPeer,
            Payload::SubmitTransactionResponse(_) => RpcApiOps::SubmitTransaction,
            Payload::GetSubnetworkResponse(_) => RpcApiOps::GetSubnetwork,
            Payload::GetVirtualSelectedParentChainFromBlockResponse(_) => RpcApiOps::GetVirtualSelectedParentChainFromBlock,
            Payload::GetBlockCountResponse(_) => RpcApiOps::GetBlockCount,
            Payload::GetBlockDagInfoResponse(_) => RpcApiOps::GetBlockDagInfo,
            Payload::ResolveFinalityConflictResponse(_) => RpcApiOps::ResolveFinalityConflict,
            Payload::GetHeadersResponse(_) => RpcApiOps::GetHeaders,
            Payload::GetUtxosByAddressesResponse(_) => RpcApiOps::GetUtxosByAddresses,
            Payload::GetBalanceByAddressResponse(_) => RpcApiOps::GetBalanceByAddress,
            Payload::GetBalancesByAddressesResponse(_) => RpcApiOps::GetBalancesByAddresses,
            Payload::GetVirtualSelectedParentBlueScoreResponse(_) => RpcApiOps::GetVirtualSelectedParentBlueScore,
            Payload::BanResponse(_) => RpcApiOps::Ban,
            Payload::UnbanResponse(_) => RpcApiOps::Unban,
            Payload::EstimateNetworkHashesPerSecondResponse(_) => RpcApiOps::EstimateNetworkHashesPerSecond,
            Payload::GetMempoolEntriesByAddressesResponse(_) => RpcApiOps::GetMempoolEntriesByAddresses,
            Payload::GetCoinSupplyResponse(_) => RpcApiOps::GetCoinSupply,
            Payload::PingResponse(_) => RpcApiOps::Ping,
            Payload::GetProcessMetricsResponse(_) => RpcApiOps::GetProcessMetrics,

            // Subscription commands for starting/stopping notifications
            Payload::NotifyBlockAddedResponse(_) => RpcApiOps::NotifyBlockAdded,
            Payload::NotifyNewBlockTemplateResponse(_) => RpcApiOps::NotifyNewBlockTemplate,

            // ???
            Payload::NotifyFinalityConflictsResponse(_) => RpcApiOps::NotifyFinalityConflicts,
            Payload::NotifyUtxosChangedResponse(_) => RpcApiOps::NotifyUtxosChanged,
            Payload::NotifyVirtualSelectedParentBlueScoreChangedResponse(_) => RpcApiOps::NotifyVirtualSelectedParentBlueScoreChanged,
            Payload::StopNotifyingPruningPointUtxoSetOverrideResponse(_) => RpcApiOps::StopNotifyingPruningPointUtxoSetOverride,
            Payload::StopNotifyingUtxosChangedResponse(_) => RpcApiOps::StopNotifyingUtxosChanged,
            Payload::NotifyPruningPointUtxoSetOverrideResponse(_) => RpcApiOps::NotifyPruningPointUtxoSetOverride,
            Payload::NotifyVirtualDaaScoreChangedResponse(_) => RpcApiOps::NotifyVirtualDaaScoreChanged,
            Payload::NotifyVirtualSelectedParentChainChangedResponse(_) => RpcApiOps::NotifyVirtualSelectedParentChainChanged,

            // Notifications
            Payload::BlockAddedNotification(_) => RpcApiOps::Notification,
            Payload::NewBlockTemplateNotification(_) => RpcApiOps::Notification,
            Payload::FinalityConflictNotification(_) => RpcApiOps::Notification,
            Payload::FinalityConflictResolvedNotification(_) => RpcApiOps::Notification,
            Payload::UtxosChangedNotification(_) => RpcApiOps::Notification,
            Payload::VirtualSelectedParentBlueScoreChangedNotification(_) => RpcApiOps::Notification,
            Payload::PruningPointUtxoSetOverrideNotification(_) => RpcApiOps::Notification,
            Payload::VirtualDaaScoreChangedNotification(_) => RpcApiOps::Notification,
            Payload::VirtualSelectedParentChainChangedNotification(_) => RpcApiOps::Notification,
        }
    }
}

impl From<kaspad_request::Payload> for KaspadRequest {
    fn from(item: kaspad_request::Payload) -> Self {
        KaspadRequest { payload: Some(item) }
    }
}

impl AsRef<KaspadRequest> for KaspadRequest {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsRef<KaspadResponse> for KaspadResponse {
    fn as_ref(&self) -> &Self {
        self
    }
}

pub mod kaspad_request_convert {
    use crate::protowire::*;
    use rpc_core::{RpcError, RpcResult};

    // impl_into_kaspad_request!(SubmitBlockRequest, SubmitBlockRequestMessage, SubmitBlockRequest);
    // impl_into_kaspad_request!(GetBlockTemplateRequest, GetBlockTemplateRequestMessage, GetBlockTemplateRequest);
    // impl_into_kaspad_request!(GetBlockRequest, GetBlockRequestMessage, GetBlockRequest);
    // impl_into_kaspad_request!(NotifyBlockAddedRequest, NotifyBlockAddedRequestMessage, NotifyBlockAddedRequest);
    // impl_into_kaspad_request!(GetInfoRequest, GetInfoRequestMessage, GetInfoRequest);
    // impl_into_kaspad_request!(
    //     NotifyNewBlockTemplateRequest,
    //     NotifyNewBlockTemplateRequestMessage,
    //     NotifyNewBlockTemplateRequest
    // );

    impl_into_kaspad_request!(Shutdown);
    impl_into_kaspad_request!(SubmitBlock);
    impl_into_kaspad_request!(GetBlockTemplate);
    impl_into_kaspad_request!(GetBlock);
    impl_into_kaspad_request!(NotifyBlockAdded);
    impl_into_kaspad_request!(GetInfo);
    impl_into_kaspad_request!(NotifyNewBlockTemplate);

    impl_into_kaspad_request!(GetCurrentNetwork);
    impl_into_kaspad_request!(GetPeerAddresses);
    impl_into_kaspad_request!(GetSelectedTipHash);
    impl_into_kaspad_request!(GetMempoolEntry);
    impl_into_kaspad_request!(GetMempoolEntries);
    impl_into_kaspad_request!(GetConnectedPeerInfo);
    impl_into_kaspad_request!(AddPeer);
    impl_into_kaspad_request!(SubmitTransaction);
    impl_into_kaspad_request!(GetSubnetwork);
    impl_into_kaspad_request!(GetVirtualSelectedParentChainFromBlock);
    impl_into_kaspad_request!(GetBlocks);
    impl_into_kaspad_request!(GetBlockCount);
    impl_into_kaspad_request!(GetBlockDagInfo);
    impl_into_kaspad_request!(ResolveFinalityConflict);
    impl_into_kaspad_request!(GetHeaders);
    impl_into_kaspad_request!(GetUtxosByAddresses);
    impl_into_kaspad_request!(GetBalanceByAddress);
    impl_into_kaspad_request!(GetBalancesByAddresses);
    impl_into_kaspad_request!(GetVirtualSelectedParentBlueScore);
    impl_into_kaspad_request!(Ban);
    impl_into_kaspad_request!(Unban);
    impl_into_kaspad_request!(EstimateNetworkHashesPerSecond);
    impl_into_kaspad_request!(GetMempoolEntriesByAddresses);
    impl_into_kaspad_request!(GetCoinSupply);
    impl_into_kaspad_request!(Ping);
    impl_into_kaspad_request!(GetProcessMetrics);

    // impl_into_kaspad_request!(StopNotifyingUtxosChanged);
    // impl_into_kaspad_request!(StopNotifyingPruningPointUtxoSetOverride);
    // impl_into_kaspad_request!(NotifyFinalityConflicts);
    // impl_into_kaspad_request!(NotifyUtxosChanged);
    // impl_into_kaspad_request!(NotifyVirtualSelectedParentBlueScoreChanged);
    // impl_into_kaspad_request!(NotifyPruningPointUtxoSetOverrideRequest);
    // impl_into_kaspad_request!(NotifyVirtualDaaScoreChangedRequest);
    // impl_into_kaspad_request!(NotifyVirtualSelectedParentChainChangedRequest);

    macro_rules! impl_into_kaspad_request {
        ($name:tt) => {
            paste::paste! {
                impl_into_kaspad_request_ex!(rpc_core::[<$name Request>],[<$name RequestMessage>],[<$name Request>]);
            }
        };
    }

    use impl_into_kaspad_request;

    macro_rules! impl_into_kaspad_request_ex {
        // ($($core_struct:ident)::+, $($protowire_struct:ident)::+, $($variant:ident)::+) => {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<&$core_struct> for kaspad_request::Payload {
                fn from(item: &$core_struct) -> Self {
                    Self::$variant(item.into())
                }
            }

            impl From<&$core_struct> for KaspadRequest {
                fn from(item: &$core_struct) -> Self {
                    Self { payload: Some(item.into()) }
                }
            }

            impl From<$core_struct> for kaspad_request::Payload {
                fn from(item: $core_struct) -> Self {
                    Self::$variant((&item).into())
                }
            }

            impl From<$core_struct> for KaspadRequest {
                fn from(item: $core_struct) -> Self {
                    Self { payload: Some((&item).into()) }
                }
            }

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&kaspad_request::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &kaspad_request::Payload) -> RpcResult<Self> {
                    if let kaspad_request::Payload::$variant(request) = item {
                        request.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($variant).to_string()))
                    }
                }
            }

            impl TryFrom<&KaspadRequest> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &KaspadRequest) -> RpcResult<Self> {
                    item.payload
                        .as_ref()
                        .ok_or(RpcError::MissingRpcFieldError("KaspaRequest".to_string(), "Payload".to_string()))?
                        .try_into()
                }
            }

            impl From<$protowire_struct> for KaspadRequest {
                fn from(item: $protowire_struct) -> Self {
                    Self { payload: Some(kaspad_request::Payload::$variant(item)) }
                }
            }

            impl From<$protowire_struct> for kaspad_request::Payload {
                fn from(item: $protowire_struct) -> Self {
                    kaspad_request::Payload::$variant(item)
                }
            }
        };
    }
    use impl_into_kaspad_request_ex;
}

pub mod kaspad_response_convert {
    use crate::protowire::*;
    use rpc_core::{RpcError, RpcResult};

    impl_into_kaspad_response!(Shutdown);
    impl_into_kaspad_response!(SubmitBlock);
    impl_into_kaspad_response!(GetBlockTemplate);
    impl_into_kaspad_response!(GetBlock);
    impl_into_kaspad_response!(GetInfo);
    impl_into_kaspad_response!(GetCurrentNetwork);

    impl_into_kaspad_response!(GetPeerAddresses);
    impl_into_kaspad_response!(GetSelectedTipHash);
    impl_into_kaspad_response!(GetMempoolEntry);
    impl_into_kaspad_response!(GetMempoolEntries);
    impl_into_kaspad_response!(GetConnectedPeerInfo);
    impl_into_kaspad_response!(AddPeer);
    impl_into_kaspad_response!(SubmitTransaction);
    impl_into_kaspad_response!(GetSubnetwork);
    impl_into_kaspad_response!(GetVirtualSelectedParentChainFromBlock);
    impl_into_kaspad_response!(GetBlocks);
    impl_into_kaspad_response!(GetBlockCount);
    impl_into_kaspad_response!(GetBlockDagInfo);
    impl_into_kaspad_response!(ResolveFinalityConflict);
    impl_into_kaspad_response!(GetHeaders);
    impl_into_kaspad_response!(GetUtxosByAddresses);
    impl_into_kaspad_response!(GetBalanceByAddress);
    impl_into_kaspad_response!(GetBalancesByAddresses);
    impl_into_kaspad_response!(GetVirtualSelectedParentBlueScore);
    impl_into_kaspad_response!(Ban);
    impl_into_kaspad_response!(Unban);
    impl_into_kaspad_response!(EstimateNetworkHashesPerSecond);
    impl_into_kaspad_response!(GetMempoolEntriesByAddresses);
    impl_into_kaspad_response!(GetCoinSupply);
    impl_into_kaspad_response!(Ping);
    impl_into_kaspad_response!(GetProcessMetrics);

    impl_into_kaspad_response!(NotifyBlockAdded);
    impl_into_kaspad_notify_response!(rpc_core::NotifyBlockAddedResponse, NotifyBlockAddedResponseMessage, NotifyBlockAddedResponse);
    impl_into_kaspad_response!(NotifyNewBlockTemplate);
    impl_into_kaspad_notify_response!(
        rpc_core::NotifyNewBlockTemplateResponse,
        NotifyNewBlockTemplateResponseMessage,
        NotifyNewBlockTemplateResponse
    );

    macro_rules! impl_into_kaspad_response {
        ($name:tt) => {
            paste::paste! {
                impl_into_kaspad_response_ex!(rpc_core::[<$name Response>],[<$name ResponseMessage>],[<$name Response>]);
            }
        };
    }

    use impl_into_kaspad_response;

    macro_rules! impl_into_kaspad_response_ex {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<RpcResult<&$core_struct>> for kaspad_response::Payload {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    kaspad_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<&$core_struct>> for KaspadResponse {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    Self { payload: Some(item.into()) }
                }
            }

            impl From<RpcResult<$core_struct>> for kaspad_response::Payload {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    kaspad_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<$core_struct>> for KaspadResponse {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    Self { payload: Some(item.into()) }
                }
            }

            impl From<RpcResult<$core_struct>> for $protowire_struct {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    item.as_ref().map_err(|x| (*x).clone()).into()
                }
            }

            impl From<RpcError> for $protowire_struct {
                fn from(item: RpcError) -> Self {
                    let x: RpcResult<&$core_struct> = Err(item);
                    x.into()
                }
            }

            impl From<$protowire_struct> for kaspad_response::Payload {
                fn from(item: $protowire_struct) -> Self {
                    kaspad_response::Payload::$variant(item)
                }
            }

            impl From<$protowire_struct> for KaspadResponse {
                fn from(item: $protowire_struct) -> Self {
                    Self { payload: Some(kaspad_response::Payload::$variant(item)) }
                }
            }

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&kaspad_response::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &kaspad_response::Payload) -> RpcResult<Self> {
                    if let kaspad_response::Payload::$variant(response) = item {
                        response.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($variant).to_string()))
                    }
                }
            }

            impl TryFrom<&KaspadResponse> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &KaspadResponse) -> RpcResult<Self> {
                    item.payload
                        .as_ref()
                        .ok_or(RpcError::MissingRpcFieldError("KaspaResponse".to_string(), "Payload".to_string()))?
                        .try_into()
                }
            }
        };
    }
    use impl_into_kaspad_response_ex;

    macro_rules! impl_into_kaspad_notify_response {
        ($($core_struct:ident)::+, $($protowire_struct:ident)::+, $($variant:ident)::+) => {

            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl<T> From<Result<(), T>> for $($protowire_struct)::+
            where
                T: Into<RpcError>,
            {
                fn from(item: Result<(), T>) -> Self {
                    item
                        .map(|_| $($core_struct)::+{})
                        .map_err(|err| err.into()).into()
                }
            }

        };
    }

    use impl_into_kaspad_notify_response;
}
