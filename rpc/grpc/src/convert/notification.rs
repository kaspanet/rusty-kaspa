use rpc_core::{Notification, RpcError, RpcResult};

use crate::protowire::{kaspad_response::Payload, BlockAddedNotificationMessage, KaspadResponse, RpcNotifyCommand};

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

impl From<&rpc_core::Notification> for KaspadResponse {
    fn from(item: &rpc_core::Notification) -> Self {
        Self { payload: Some(item.into()) }
    }
}

impl From<&rpc_core::Notification> for Payload {
    fn from(item: &rpc_core::Notification) -> Self {
        match item {
            Notification::BlockAdded(ref notif) => Payload::BlockAddedNotification(notif.into()),
            Notification::VirtualSelectedParentChainChanged(_) => todo!(),
            Notification::FinalityConflict(_) => todo!(),
            Notification::FinalityConflictResolved(_) => todo!(),
            Notification::UtxosChanged(_) => todo!(),
            Notification::VirtualSelectedParentBlueScoreChanged(_) => todo!(),
            Notification::VirtualDaaScoreChanged(_) => todo!(),
            Notification::PruningPointUTXOSetOverride(_) => todo!(),
            Notification::NewBlockTemplate(_) => todo!(),
        }
    }
}

impl From<&rpc_core::BlockAddedNotification> for BlockAddedNotificationMessage {
    fn from(item: &rpc_core::BlockAddedNotification) -> Self {
        Self { block: Some((&item.block).into()) }
    }
}

impl From<rpc_core::api::ops::SubscribeCommand> for RpcNotifyCommand {
    fn from(item: rpc_core::api::ops::SubscribeCommand) -> Self {
        match item {
            rpc_core::api::ops::SubscribeCommand::Start => RpcNotifyCommand::NotifyStart,
            rpc_core::api::ops::SubscribeCommand::Stop => RpcNotifyCommand::NotifyStop,
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

impl TryFrom<&KaspadResponse> for rpc_core::Notification {
    type Error = RpcError;
    fn try_from(item: &KaspadResponse) -> Result<Self, Self::Error> {
        match item.payload {
            Some(ref payload) => Ok(payload.try_into()?),
            None => Err(RpcError::MissingRpcFieldError("KaspadResponse".to_string(), "payload".to_string())),
        }
    }
}

impl TryFrom<&Payload> for rpc_core::Notification {
    type Error = RpcError;
    fn try_from(item: &Payload) -> Result<Self, Self::Error> {
        match item {
            Payload::BlockAddedNotification(ref notif) => Ok(Notification::BlockAdded(notif.try_into()?)),
            _ => Err(RpcError::NotImplemented),
        }
    }
}

impl TryFrom<&BlockAddedNotificationMessage> for rpc_core::BlockAddedNotification {
    type Error = RpcError;
    fn try_from(item: &BlockAddedNotificationMessage) -> RpcResult<Self> {
        if let Some(ref block) = item.block {
            Ok(Self { block: block.try_into()? })
        } else {
            Err(RpcError::MissingRpcFieldError("BlockAddedNotificationMessage".to_string(), "block".to_string()))
        }
    }
}

impl From<RpcNotifyCommand> for rpc_core::api::ops::SubscribeCommand {
    fn from(item: RpcNotifyCommand) -> Self {
        match item {
            RpcNotifyCommand::NotifyStart => rpc_core::api::ops::SubscribeCommand::Start,
            RpcNotifyCommand::NotifyStop => rpc_core::api::ops::SubscribeCommand::Stop,
        }
    }
}
