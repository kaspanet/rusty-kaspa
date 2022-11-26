use crate::protowire::{kaspad_request, kaspad_response, KaspadRequest, KaspadResponse};
use rpc_core::api::ops::RpcApiOps;

impl From<&kaspad_request::Payload> for RpcApiOps {
    fn from(item: &kaspad_request::Payload) -> Self {
        match item {
            kaspad_request::Payload::SubmitBlockRequest(_) => RpcApiOps::SubmitBlock,
            kaspad_request::Payload::GetBlockTemplateRequest(_) => RpcApiOps::GetBlockTemplate,
            kaspad_request::Payload::GetCurrentNetworkRequest(_) => RpcApiOps::GetCurrentNetwork,
            kaspad_request::Payload::GetBlockRequest(_) => RpcApiOps::GetBlock,
            kaspad_request::Payload::GetInfoRequest(_) => RpcApiOps::GetInfo,

            // Subscription commands for starting/stopping notifications
            kaspad_request::Payload::NotifyBlockAddedRequest(_) => RpcApiOps::NotifyBlockAdded,
        }
    }
}

impl From<&kaspad_response::Payload> for RpcApiOps {
    fn from(item: &kaspad_response::Payload) -> Self {
        match item {
            kaspad_response::Payload::SubmitBlockResponse(_) => RpcApiOps::SubmitBlock,
            kaspad_response::Payload::GetBlockTemplateResponse(_) => RpcApiOps::GetBlockTemplate,
            kaspad_response::Payload::GetCurrentNetworkResponse(_) => RpcApiOps::GetCurrentNetwork,
            kaspad_response::Payload::GetBlockResponse(_) => RpcApiOps::GetBlock,
            kaspad_response::Payload::GetInfoResponse(_) => RpcApiOps::GetInfo,

            // Subscription commands for starting/stopping notifications
            kaspad_response::Payload::NotifyBlockAddedResponse(_) => RpcApiOps::NotifyBlockAdded,

            // Notifications
            kaspad_response::Payload::BlockAddedNotification(_) => RpcApiOps::Notification,
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

    impl_into_kaspad_request!(rpc_core::SubmitBlockRequest, SubmitBlockRequestMessage, SubmitBlockRequest);
    impl_into_kaspad_request!(rpc_core::GetBlockTemplateRequest, GetBlockTemplateRequestMessage, GetBlockTemplateRequest);
    impl_into_kaspad_request!(rpc_core::GetBlockRequest, GetBlockRequestMessage, GetBlockRequest);
    impl_into_kaspad_request!(rpc_core::NotifyBlockAddedRequest, NotifyBlockAddedRequestMessage, NotifyBlockAddedRequest);
    impl_into_kaspad_request!(rpc_core::GetInfoRequest, GetInfoRequestMessage, GetInfoRequest);

    macro_rules! impl_into_kaspad_request {
        ($($core_struct:ident)::+, $($protowire_struct:ident)::+, $($variant:ident)::+) => {

            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<&$($core_struct)::+> for kaspad_request::Payload {
                fn from(item: &$($core_struct)::+) -> Self {
                    Self::$($variant)::+(item.into())
                }
            }

            impl From<&$($core_struct)::+> for KaspadRequest {
                fn from(item: &$($core_struct)::+) -> Self {
                    Self {
                        payload: Some(item.into())
                    }
                }
            }

            impl From<$($core_struct)::+> for kaspad_request::Payload {
                fn from(item: $($core_struct)::+) -> Self {
                    Self::$($variant)::+((&item).into())
                }
            }

            impl From<$($core_struct)::+> for KaspadRequest {
                fn from(item: $($core_struct)::+) -> Self {
                    Self {
                        payload: Some((&item).into())
                    }
                }
            }

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&kaspad_request::Payload> for $($core_struct)::+ {
                type Error = RpcError;
                fn try_from(item: &kaspad_request::Payload) -> RpcResult<Self> {
                    if let kaspad_request::Payload::$($variant)::+(request) = item {
                        request.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($($variant)::+).to_string()))
                    }
                }
            }

            impl TryFrom<&KaspadRequest> for $($core_struct)::+ {
                type Error = RpcError;
                fn try_from(item: &KaspadRequest) -> RpcResult<Self> {
                    item.payload
                        .as_ref()
                        .ok_or(RpcError::MissingRpcFieldError("KaspaRequest".to_string(), "Payload".to_string()))?
                        .try_into()
                }
            }

            impl From<$($protowire_struct)::+> for KaspadRequest {
                fn from(item: $($protowire_struct)::+) -> Self {
                    Self { payload: Some(kaspad_request::Payload::$($variant)::+(item)) }
                }
            }

            impl From<$($protowire_struct)::+> for kaspad_request::Payload {
                fn from(item: $($protowire_struct)::+) -> Self {
                    kaspad_request::Payload::$($variant)::+(item)
                }
            }

        };
    }
    use impl_into_kaspad_request;
}

pub mod kaspad_response_convert {
    use crate::protowire::*;
    use rpc_core::{RpcError, RpcResult};

    impl_into_kaspad_response!(rpc_core::SubmitBlockResponse, SubmitBlockResponseMessage, SubmitBlockResponse);
    impl_into_kaspad_response!(rpc_core::GetBlockTemplateResponse, GetBlockTemplateResponseMessage, GetBlockTemplateResponse);
    impl_into_kaspad_response!(rpc_core::GetBlockResponse, GetBlockResponseMessage, GetBlockResponse);
    impl_into_kaspad_response!(rpc_core::GetInfoResponse, GetInfoResponseMessage, GetInfoResponse);

    impl_into_kaspad_response!(rpc_core::NotifyBlockAddedResponse, NotifyBlockAddedResponseMessage, NotifyBlockAddedResponse);
    impl_into_kaspad_notify_response!(rpc_core::NotifyBlockAddedResponse, NotifyBlockAddedResponseMessage, NotifyBlockAddedResponse);

    macro_rules! impl_into_kaspad_response {
        ($($core_struct:ident)::+, $($protowire_struct:ident)::+, $($variant:ident)::+) => {

            // ----------------------------------------------------------------------------
            // rpc_core to protowire
            // ----------------------------------------------------------------------------

            impl From<RpcResult<&$($core_struct)::+>> for kaspad_response::Payload {
                fn from(item: RpcResult<&$($core_struct)::+>) -> Self {
                    kaspad_response::Payload::$($variant)::+(item.into())
                }
            }

            impl From<RpcResult<&$($core_struct)::+>> for KaspadResponse {
                fn from(item: RpcResult<&$($core_struct)::+>) -> Self {
                    Self {
                        payload: Some(item.into())
                    }
                }
            }

            impl From<RpcResult<$($core_struct)::+>> for kaspad_response::Payload {
                fn from(item: RpcResult<$($core_struct)::+>) -> Self {
                    kaspad_response::Payload::$($variant)::+(item.into())
                }
            }

            impl From<RpcResult<$($core_struct)::+>> for KaspadResponse {
                fn from(item: RpcResult<$($core_struct)::+>) -> Self {
                    Self {
                        payload: Some(item.into())
                    }
                }
            }

            impl From<RpcResult<$($core_struct)::+>> for $($protowire_struct)::+ {
                fn from(item: RpcResult<$($core_struct)::+>) -> Self {
                    item.as_ref().map_err(|x| (*x).clone()).into()
                }
            }

            impl From<RpcError> for $($protowire_struct)::+ {
                fn from(item: RpcError) -> Self {
                    let x: RpcResult<&$($core_struct)::+> = Err(item);
                    x.into()
                }
            }

            impl From<$($protowire_struct)::+> for kaspad_response::Payload {
                fn from(item: $($protowire_struct)::+) -> Self {
                    kaspad_response::Payload::$($variant)::+(item)
                }
            }

            impl From<$($protowire_struct)::+> for KaspadResponse {
                fn from(item: $($protowire_struct)::+) -> Self {
                    Self {
                        payload: Some(kaspad_response::Payload::$($variant)::+(item))
                    }
                }
            }

            // ----------------------------------------------------------------------------
            // protowire to rpc_core
            // ----------------------------------------------------------------------------

            impl TryFrom<&kaspad_response::Payload> for $($core_struct)::+ {
                type Error = RpcError;
                fn try_from(item: &kaspad_response::Payload) -> RpcResult<Self> {
                    if let kaspad_response::Payload::$($variant)::+(response) = item {
                        response.try_into()
                    } else {
                        Err(RpcError::MissingRpcFieldError("Payload".to_string(), stringify!($($variant)::+).to_string()))
                    }
                }
            }

            impl TryFrom<&KaspadResponse> for $($core_struct)::+ {
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
    use impl_into_kaspad_response;

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
