use crate::protowire::{kaspad_request, KaspadRequest, KaspadResponse};

impl From<kaspad_request::Payload> for KaspadRequest {
    fn from(item: kaspad_request::Payload) -> Self {
        KaspadRequest { id: 0, payload: Some(item) }
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
    use kaspa_rpc_core::{RpcError, RpcResult};

    impl_into_kaspad_request!(Shutdown);
    impl_into_kaspad_request!(SubmitBlock);
    impl_into_kaspad_request!(GetBlockTemplate);
    impl_into_kaspad_request!(GetBlock);
    impl_into_kaspad_request!(GetInfo);

    impl_into_kaspad_request!(GetCurrentNetwork);
    impl_into_kaspad_request!(GetPeerAddresses);
    impl_into_kaspad_request!(GetSink);
    impl_into_kaspad_request!(GetMempoolEntry);
    impl_into_kaspad_request!(GetMempoolEntries);
    impl_into_kaspad_request!(GetConnectedPeerInfo);
    impl_into_kaspad_request!(AddPeer);
    impl_into_kaspad_request!(SubmitTransaction);
    impl_into_kaspad_request!(SubmitTransactionReplacement);
    impl_into_kaspad_request!(GetSubnetwork);
    impl_into_kaspad_request!(GetVirtualChainFromBlock);
    impl_into_kaspad_request!(GetBlocks);
    impl_into_kaspad_request!(GetBlockCount);
    impl_into_kaspad_request!(GetBlockDagInfo);
    impl_into_kaspad_request!(ResolveFinalityConflict);
    impl_into_kaspad_request!(GetHeaders);
    impl_into_kaspad_request!(GetUtxosByAddresses);
    impl_into_kaspad_request!(GetBalanceByAddress);
    impl_into_kaspad_request!(GetBalancesByAddresses);
    impl_into_kaspad_request!(GetSinkBlueScore);
    impl_into_kaspad_request!(Ban);
    impl_into_kaspad_request!(Unban);
    impl_into_kaspad_request!(EstimateNetworkHashesPerSecond);
    impl_into_kaspad_request!(GetMempoolEntriesByAddresses);
    impl_into_kaspad_request!(GetCoinSupply);
    impl_into_kaspad_request!(Ping);
    impl_into_kaspad_request!(GetMetrics);
    impl_into_kaspad_request!(GetConnections);
    impl_into_kaspad_request!(GetSystemInfo);
    impl_into_kaspad_request!(GetServerInfo);
    impl_into_kaspad_request!(GetSyncStatus);
    impl_into_kaspad_request!(GetDaaScoreTimestampEstimate);
    impl_into_kaspad_request!(GetFeeEstimate);
    impl_into_kaspad_request!(GetFeeEstimateExperimental);
    impl_into_kaspad_request!(GetCurrentBlockColor);

    impl_into_kaspad_request!(NotifyBlockAdded);
    impl_into_kaspad_request!(NotifyNewBlockTemplate);
    impl_into_kaspad_request!(NotifyUtxosChanged);
    impl_into_kaspad_request!(NotifyPruningPointUtxoSetOverride);
    impl_into_kaspad_request!(NotifyFinalityConflict);
    impl_into_kaspad_request!(NotifyVirtualDaaScoreChanged);
    impl_into_kaspad_request!(NotifyVirtualChainChanged);
    impl_into_kaspad_request!(NotifySinkBlueScoreChanged);

    macro_rules! impl_into_kaspad_request {
        ($name:tt) => {
            paste::paste! {
                impl_into_kaspad_request_ex!(kaspa_rpc_core::[<$name Request>],[<$name RequestMessage>],[<$name Request>]);
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
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<$core_struct> for kaspad_request::Payload {
                fn from(item: $core_struct) -> Self {
                    Self::$variant((&item).into())
                }
            }

            impl From<$core_struct> for KaspadRequest {
                fn from(item: $core_struct) -> Self {
                    Self { id: 0, payload: Some((&item).into()) }
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
                    Self { id: 0, payload: Some(kaspad_request::Payload::$variant(item)) }
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
    use kaspa_rpc_core::{RpcError, RpcResult};

    impl_into_kaspad_response!(Shutdown);
    impl_into_kaspad_response!(SubmitBlock);
    impl_into_kaspad_response!(GetBlockTemplate);
    impl_into_kaspad_response!(GetBlock);
    impl_into_kaspad_response!(GetInfo);
    impl_into_kaspad_response!(GetCurrentNetwork);

    impl_into_kaspad_response!(GetPeerAddresses);
    impl_into_kaspad_response!(GetSink);
    impl_into_kaspad_response!(GetMempoolEntry);
    impl_into_kaspad_response!(GetMempoolEntries);
    impl_into_kaspad_response!(GetConnectedPeerInfo);
    impl_into_kaspad_response!(AddPeer);
    impl_into_kaspad_response!(SubmitTransaction);
    impl_into_kaspad_response!(SubmitTransactionReplacement);
    impl_into_kaspad_response!(GetSubnetwork);
    impl_into_kaspad_response!(GetVirtualChainFromBlock);
    impl_into_kaspad_response!(GetBlocks);
    impl_into_kaspad_response!(GetBlockCount);
    impl_into_kaspad_response!(GetBlockDagInfo);
    impl_into_kaspad_response!(ResolveFinalityConflict);
    impl_into_kaspad_response!(GetHeaders);
    impl_into_kaspad_response!(GetUtxosByAddresses);
    impl_into_kaspad_response!(GetBalanceByAddress);
    impl_into_kaspad_response!(GetBalancesByAddresses);
    impl_into_kaspad_response!(GetSinkBlueScore);
    impl_into_kaspad_response!(Ban);
    impl_into_kaspad_response!(Unban);
    impl_into_kaspad_response!(EstimateNetworkHashesPerSecond);
    impl_into_kaspad_response!(GetMempoolEntriesByAddresses);
    impl_into_kaspad_response!(GetCoinSupply);
    impl_into_kaspad_response!(Ping);
    impl_into_kaspad_response!(GetMetrics);
    impl_into_kaspad_response!(GetConnections);
    impl_into_kaspad_response!(GetSystemInfo);
    impl_into_kaspad_response!(GetServerInfo);
    impl_into_kaspad_response!(GetSyncStatus);
    impl_into_kaspad_response!(GetDaaScoreTimestampEstimate);
    impl_into_kaspad_response!(GetFeeEstimate);
    impl_into_kaspad_response!(GetFeeEstimateExperimental);
    impl_into_kaspad_response!(GetCurrentBlockColor);

    impl_into_kaspad_notify_response!(NotifyBlockAdded);
    impl_into_kaspad_notify_response!(NotifyNewBlockTemplate);
    impl_into_kaspad_notify_response!(NotifyUtxosChanged);
    impl_into_kaspad_notify_response!(NotifyPruningPointUtxoSetOverride);
    impl_into_kaspad_notify_response!(NotifyFinalityConflict);
    impl_into_kaspad_notify_response!(NotifyVirtualDaaScoreChanged);
    impl_into_kaspad_notify_response!(NotifyVirtualChainChanged);
    impl_into_kaspad_notify_response!(NotifySinkBlueScoreChanged);

    impl_into_kaspad_notify_response!(NotifyUtxosChanged, StopNotifyingUtxosChanged);
    impl_into_kaspad_notify_response!(NotifyPruningPointUtxoSetOverride, StopNotifyingPruningPointUtxoSetOverride);

    macro_rules! impl_into_kaspad_response {
        ($name:tt) => {
            paste::paste! {
                impl_into_kaspad_response_ex!(kaspa_rpc_core::[<$name Response>],[<$name ResponseMessage>],[<$name Response>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            paste::paste! {
                impl_into_kaspad_response_base!(kaspa_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>],[<$protowire_name Response>]);
            }
        };
    }
    use impl_into_kaspad_response;

    macro_rules! impl_into_kaspad_response_base {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

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
                    Self { id: 0, payload: Some(kaspad_response::Payload::$variant(item)) }
                }
            }
        };
    }
    use impl_into_kaspad_response_base;

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
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<RpcResult<$core_struct>> for kaspad_response::Payload {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    kaspad_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<$core_struct>> for KaspadResponse {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl_into_kaspad_response_base!($core_struct, $protowire_struct, $variant);

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
        ($name:tt) => {
            impl_into_kaspad_response!($name);

            paste::paste! {
                impl_into_kaspad_notify_response_ex!(kaspa_rpc_core::[<$name Response>],[<$name ResponseMessage>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            impl_into_kaspad_response!($core_name, $protowire_name);

            paste::paste! {
                impl_into_kaspad_notify_response_ex!(kaspa_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>]);
            }
        };
    }
    use impl_into_kaspad_notify_response;

    macro_rules! impl_into_kaspad_notify_response_ex {
        ($($core_struct:ident)::+, $protowire_struct:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl<T> From<Result<(), T>> for $protowire_struct
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
    use impl_into_kaspad_notify_response_ex;
}
