use rpc_core::{api::ops::SubscribeCommand, NotificationType};

use crate::protowire::{
    kaspad_request, kaspad_response, KaspadRequest, KaspadResponse, NotifyBlockAddedRequestMessage,
    NotifyNewBlockTemplateRequestMessage,
};

impl KaspadRequest {
    pub fn from_notification_type(notification_type: &NotificationType, command: SubscribeCommand) -> Self {
        KaspadRequest { payload: Some(kaspad_request::Payload::from_notification_type(notification_type, command)) }
    }
}

impl kaspad_request::Payload {
    pub fn from_notification_type(notification_type: &NotificationType, command: SubscribeCommand) -> Self {
        match notification_type {
            NotificationType::BlockAdded => {
                kaspad_request::Payload::NotifyBlockAddedRequest(NotifyBlockAddedRequestMessage { command: command.into() })
            },
            NotificationType::NewBlockTemplate => {
                kaspad_request::Payload::NotifyNewBlockTemplateRequest(NotifyNewBlockTemplateRequestMessage { command: command.into() })
            },

            // TODO: implement all other notifications
            _ => {
                kaspad_request::Payload::NotifyBlockAddedRequest(NotifyBlockAddedRequestMessage { command: command.into() })
            }
            // NotificationType::VirtualSelectedParentChainChanged => todo!(),
            // NotificationType::FinalityConflicts => todo!(),
            // NotificationType::FinalityConflictResolved => todo!(),
            // NotificationType::UtxosChanged(_) => todo!(),
            // NotificationType::VirtualSelectedParentBlueScoreChanged => todo!(),
            // NotificationType::VirtualDaaScoreChanged => todo!(),
            // NotificationType::PruningPointUTXOSetOverride => todo!(),
            // NotificationType::NewBlockTemplate => todo!(),
        }
    }
}

impl KaspadResponse {
    pub fn is_notification(&self) -> bool {
        match self.payload {
            Some(ref payload) => payload.is_notification(),
            None => false,
        }
    }
}

#[allow(clippy::match_like_matches_macro)]
impl kaspad_response::Payload {
    pub fn is_notification(&self) -> bool {
        match self {
            kaspad_response::Payload::BlockAddedNotification(_) => true,
            _ => false,
        }
    }
}
