use crate::protowire::{AerodRequest, AerodResponse, aerod_request};

impl From<aerod_request::Payload> for AerodRequest {
    fn from(item: aerod_request::Payload) -> Self {
        AerodRequest { id: 0, payload: Some(item) }
    }
}

impl AsRef<AerodRequest> for AerodRequest {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl AsRef<AerodResponse> for AerodResponse {
    fn as_ref(&self) -> &Self {
        self
    }
}

pub mod aerod_request_convert {
    use crate::protowire::*;
    use aero_rpc_core::{RpcError, RpcResult};

    impl_into_aerod_request!(Shutdown);
    impl_into_aerod_request!(SubmitBlock);
    impl_into_aerod_request!(GetBlockTemplate);
    impl_into_aerod_request!(GetBlock);
    impl_into_aerod_request!(GetInfo);

    impl_into_aerod_request!(GetCurrentNetwork);
    impl_into_aerod_request!(GetPeerAddresses);
    impl_into_aerod_request!(GetSink);
    impl_into_aerod_request!(GetMempoolEntry);
    impl_into_aerod_request!(GetMempoolEntries);
    impl_into_aerod_request!(GetConnectedPeerInfo);
    impl_into_aerod_request!(AddPeer);
    impl_into_aerod_request!(SubmitTransaction);
    impl_into_aerod_request!(SubmitTransactionReplacement);
    impl_into_aerod_request!(GetSubnetwork);
    impl_into_aerod_request!(GetVirtualChainFromBlock);
    impl_into_aerod_request!(GetBlocks);
    impl_into_aerod_request!(GetBlockCount);
    impl_into_aerod_request!(GetBlockDagInfo);
    impl_into_aerod_request!(ResolveFinalityConflict);
    impl_into_aerod_request!(GetHeaders);
    impl_into_aerod_request!(GetUtxosByAddresses);
    impl_into_aerod_request!(GetBalanceByAddress);
    impl_into_aerod_request!(GetBalancesByAddresses);
    impl_into_aerod_request!(GetSinkBlueScore);
    impl_into_aerod_request!(Ban);
    impl_into_aerod_request!(Unban);
    impl_into_aerod_request!(EstimateNetworkHashesPerSecond);
    impl_into_aerod_request!(GetMempoolEntriesByAddresses);
    impl_into_aerod_request!(GetCoinSupply);
    impl_into_aerod_request!(Ping);
    impl_into_aerod_request!(GetMetrics);
    impl_into_aerod_request!(GetConnections);
    impl_into_aerod_request!(GetSystemInfo);
    impl_into_aerod_request!(GetServerInfo);
    impl_into_aerod_request!(GetSyncStatus);
    impl_into_aerod_request!(GetDaaScoreTimestampEstimate);
    impl_into_aerod_request!(GetFeeEstimate);
    impl_into_aerod_request!(GetFeeEstimateExperimental);
    impl_into_aerod_request!(GetCurrentBlockColor);
    impl_into_aerod_request!(GetUtxoReturnAddress);
    impl_into_aerod_request!(GetVirtualChainFromBlockV2);
    impl_into_aerod_request!(GetBlockRewardInfo);
    impl_into_aerod_request!(GetSeqCommitLaneProof);

    impl_into_aerod_request!(NotifyBlockAdded);
    impl_into_aerod_request!(NotifyNewBlockTemplate);
    impl_into_aerod_request!(NotifyUtxosChanged);
    impl_into_aerod_request!(NotifyPruningPointUtxoSetOverride);
    impl_into_aerod_request!(NotifyFinalityConflict);
    impl_into_aerod_request!(NotifyVirtualDaaScoreChanged);
    impl_into_aerod_request!(NotifyVirtualChainChanged);
    impl_into_aerod_request!(NotifySinkBlueScoreChanged);

    macro_rules! impl_into_aerod_request {
        ($name:tt) => {
            paste::paste! {
                impl_into_aerod_request_ex!(aero_rpc_core::[<$name Request>],[<$name RequestMessage>],[<$name Request>]);
            }
        };
    }

    use impl_into_aerod_request;

    macro_rules! impl_into_aerod_request_ex {
        // ($($core_struct:ident)::+, $($protowire_struct:ident)::+, $($variant:ident)::+) => {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<&$core_struct> for aerod_request::Payload {
                fn from(item: &$core_struct) -> Self {
                    Self::$variant(item.into())
                }
            }

            impl From<&$core_struct> for AerodRequest {
                fn from(item: &$core_struct) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<$core_struct> for aerod_request::Payload {
                fn from(item: $core_struct) -> Self {
                    Self::$variant((&item).into())
                }
            }

            impl From<$core_struct> for AerodRequest {
                fn from(item: $core_struct) -> Self {
                    Self { id: 0, payload: Some((&item).into()) }
                }
            }

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&aerod_request::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &aerod_request::Payload) -> RpcResult<Self> {
                    if let aerod_request::Payload::$variant(request) = item {
                        request.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($variant).to_string()))
                    }
                }
            }

            impl TryFrom<&AerodRequest> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &AerodRequest) -> RpcResult<Self> {
                    item.payload
                        .as_ref()
                        .ok_or(RpcError::MissingRpcFieldError("AeroRequest".to_string(), "Payload".to_string()))?
                        .try_into()
                }
            }

            impl From<$protowire_struct> for AerodRequest {
                fn from(item: $protowire_struct) -> Self {
                    Self { id: 0, payload: Some(aerod_request::Payload::$variant(item)) }
                }
            }

            impl From<$protowire_struct> for aerod_request::Payload {
                fn from(item: $protowire_struct) -> Self {
                    aerod_request::Payload::$variant(item)
                }
            }
        };
    }
    use impl_into_aerod_request_ex;
}

pub mod aerod_response_convert {
    use crate::protowire::*;
    use aero_rpc_core::{RpcError, RpcResult};

    impl_into_aerod_response!(Shutdown);
    impl_into_aerod_response!(SubmitBlock);
    impl_into_aerod_response!(GetBlockTemplate);
    impl_into_aerod_response!(GetBlock);
    impl_into_aerod_response!(GetInfo);
    impl_into_aerod_response!(GetCurrentNetwork);

    impl_into_aerod_response!(GetPeerAddresses);
    impl_into_aerod_response!(GetSink);
    impl_into_aerod_response!(GetMempoolEntry);
    impl_into_aerod_response!(GetMempoolEntries);
    impl_into_aerod_response!(GetConnectedPeerInfo);
    impl_into_aerod_response!(AddPeer);
    impl_into_aerod_response!(SubmitTransaction);
    impl_into_aerod_response!(SubmitTransactionReplacement);
    impl_into_aerod_response!(GetSubnetwork);
    impl_into_aerod_response!(GetVirtualChainFromBlock);
    impl_into_aerod_response!(GetBlocks);
    impl_into_aerod_response!(GetBlockCount);
    impl_into_aerod_response!(GetBlockDagInfo);
    impl_into_aerod_response!(ResolveFinalityConflict);
    impl_into_aerod_response!(GetHeaders);
    impl_into_aerod_response!(GetUtxosByAddresses);
    impl_into_aerod_response!(GetBalanceByAddress);
    impl_into_aerod_response!(GetBalancesByAddresses);
    impl_into_aerod_response!(GetSinkBlueScore);
    impl_into_aerod_response!(Ban);
    impl_into_aerod_response!(Unban);
    impl_into_aerod_response!(EstimateNetworkHashesPerSecond);
    impl_into_aerod_response!(GetMempoolEntriesByAddresses);
    impl_into_aerod_response!(GetCoinSupply);
    impl_into_aerod_response!(Ping);
    impl_into_aerod_response!(GetMetrics);
    impl_into_aerod_response!(GetConnections);
    impl_into_aerod_response!(GetSystemInfo);
    impl_into_aerod_response!(GetServerInfo);
    impl_into_aerod_response!(GetSyncStatus);
    impl_into_aerod_response!(GetDaaScoreTimestampEstimate);
    impl_into_aerod_response!(GetFeeEstimate);
    impl_into_aerod_response!(GetFeeEstimateExperimental);
    impl_into_aerod_response!(GetCurrentBlockColor);
    impl_into_aerod_response!(GetUtxoReturnAddress);
    impl_into_aerod_response!(GetVirtualChainFromBlockV2);
    impl_into_aerod_response!(GetBlockRewardInfo);
    impl_into_aerod_response!(GetSeqCommitLaneProof);

    impl_into_aerod_notify_response!(NotifyBlockAdded);
    impl_into_aerod_notify_response!(NotifyNewBlockTemplate);
    impl_into_aerod_notify_response!(NotifyUtxosChanged);
    impl_into_aerod_notify_response!(NotifyPruningPointUtxoSetOverride);
    impl_into_aerod_notify_response!(NotifyFinalityConflict);
    impl_into_aerod_notify_response!(NotifyVirtualDaaScoreChanged);
    impl_into_aerod_notify_response!(NotifyVirtualChainChanged);
    impl_into_aerod_notify_response!(NotifySinkBlueScoreChanged);

    impl_into_aerod_notify_response!(NotifyUtxosChanged, StopNotifyingUtxosChanged);
    impl_into_aerod_notify_response!(NotifyPruningPointUtxoSetOverride, StopNotifyingPruningPointUtxoSetOverride);

    macro_rules! impl_into_aerod_response {
        ($name:tt) => {
            paste::paste! {
                impl_into_aerod_response_ex!(aero_rpc_core::[<$name Response>],[<$name ResponseMessage>],[<$name Response>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            paste::paste! {
                impl_into_aerod_response_base!(aero_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>],[<$protowire_name Response>]);
            }
        };
    }
    use impl_into_aerod_response;

    macro_rules! impl_into_aerod_response_base {
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

            impl From<$protowire_struct> for aerod_response::Payload {
                fn from(item: $protowire_struct) -> Self {
                    aerod_response::Payload::$variant(item)
                }
            }

            impl From<$protowire_struct> for AerodResponse {
                fn from(item: $protowire_struct) -> Self {
                    Self { id: 0, payload: Some(aerod_response::Payload::$variant(item)) }
                }
            }
        };
    }
    use impl_into_aerod_response_base;

    macro_rules! impl_into_aerod_response_ex {
        ($core_struct:path, $protowire_struct:ident, $variant:ident) => {
            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<RpcResult<&$core_struct>> for aerod_response::Payload {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    aerod_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<&$core_struct>> for AerodResponse {
                fn from(item: RpcResult<&$core_struct>) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl From<RpcResult<$core_struct>> for aerod_response::Payload {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    aerod_response::Payload::$variant(item.into())
                }
            }

            impl From<RpcResult<$core_struct>> for AerodResponse {
                fn from(item: RpcResult<$core_struct>) -> Self {
                    Self { id: 0, payload: Some(item.into()) }
                }
            }

            impl_into_aerod_response_base!($core_struct, $protowire_struct, $variant);

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&aerod_response::Payload> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &aerod_response::Payload) -> RpcResult<Self> {
                    if let aerod_response::Payload::$variant(response) = item {
                        response.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($variant).to_string()))
                    }
                }
            }

            impl TryFrom<&AerodResponse> for $core_struct {
                type Error = RpcError;
                fn try_from(item: &AerodResponse) -> RpcResult<Self> {
                    item.payload
                        .as_ref()
                        .ok_or(RpcError::MissingRpcFieldError("AeroResponse".to_string(), "Payload".to_string()))?
                        .try_into()
                }
            }
        };
    }
    use impl_into_aerod_response_ex;

    macro_rules! impl_into_aerod_notify_response {
        ($name:tt) => {
            impl_into_aerod_response!($name);

            paste::paste! {
                impl_into_aerod_notify_response_ex!(aero_rpc_core::[<$name Response>],[<$name ResponseMessage>]);
            }
        };
        ($core_name:tt, $protowire_name:tt) => {
            impl_into_aerod_response!($core_name, $protowire_name);

            paste::paste! {
                impl_into_aerod_notify_response_ex!(aero_rpc_core::[<$core_name Response>],[<$protowire_name ResponseMessage>]);
            }
        };
    }
    use impl_into_aerod_notify_response;

    macro_rules! impl_into_aerod_notify_response_ex {
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
    use impl_into_aerod_notify_response_ex;
}
