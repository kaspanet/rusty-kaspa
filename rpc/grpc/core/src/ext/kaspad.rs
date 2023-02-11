use kaspa_rpc_core::{api::ops::SubscribeCommand, NotificationType};

use crate::protowire::{
    kaspad_request, kaspad_response, KaspadRequest, KaspadResponse, NotifyBlockAddedRequestMessage,
    NotifyFinalityConflictRequestMessage, NotifyNewBlockTemplateRequestMessage, NotifyPruningPointUtxoSetOverrideRequestMessage,
    NotifyUtxosChangedRequestMessage, NotifyVirtualDaaScoreChangedRequestMessage,
    NotifyVirtualSelectedParentBlueScoreChangedRequestMessage, NotifyVirtualSelectedParentChainChangedRequestMessage,
};

impl KaspadRequest {
    pub fn from_notification_type(notification_type: &NotificationType, command: SubscribeCommand) -> Self {
        KaspadRequest { id: 0, payload: Some(kaspad_request::Payload::from_notification_type(notification_type, command)) }
    }
}

impl kaspad_request::Payload {
    pub fn from_notification_type(notification_type: &NotificationType, command: SubscribeCommand) -> Self {
        match notification_type {
            NotificationType::BlockAdded => {
                kaspad_request::Payload::NotifyBlockAddedRequest(NotifyBlockAddedRequestMessage { command: command.into() })
            }
            NotificationType::NewBlockTemplate => {
                kaspad_request::Payload::NotifyNewBlockTemplateRequest(NotifyNewBlockTemplateRequestMessage {
                    command: command.into(),
                })
            }

            NotificationType::VirtualSelectedParentChainChanged(ref include_accepted_transaction_ids) => {
                kaspad_request::Payload::NotifyVirtualSelectedParentChainChangedRequest(
                    NotifyVirtualSelectedParentChainChangedRequestMessage {
                        command: command.into(),
                        include_accepted_transaction_ids: *include_accepted_transaction_ids,
                    },
                )
            }
            NotificationType::FinalityConflict => {
                kaspad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                    command: command.into(),
                })
            }
            NotificationType::FinalityConflictResolved => {
                kaspad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                    command: command.into(),
                })
            }
            NotificationType::UtxosChanged(ref addresses) => {
                kaspad_request::Payload::NotifyUtxosChangedRequest(NotifyUtxosChangedRequestMessage {
                    addresses: addresses.iter().map(|x| x.into()).collect::<Vec<String>>(),
                    command: command.into(),
                })
            }
            NotificationType::VirtualSelectedParentBlueScoreChanged => {
                kaspad_request::Payload::NotifyVirtualSelectedParentBlueScoreChangedRequest(
                    NotifyVirtualSelectedParentBlueScoreChangedRequestMessage { command: command.into() },
                )
            }
            NotificationType::VirtualDaaScoreChanged => {
                kaspad_request::Payload::NotifyVirtualDaaScoreChangedRequest(NotifyVirtualDaaScoreChangedRequestMessage {
                    command: command.into(),
                })
            }
            NotificationType::PruningPointUtxoSetOverride => {
                kaspad_request::Payload::NotifyPruningPointUtxoSetOverrideRequest(NotifyPruningPointUtxoSetOverrideRequestMessage {
                    command: command.into(),
                })
            }
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
        use crate::protowire::kaspad_response::Payload;
        match self {
            Payload::BlockAddedNotification(_) => true,
            Payload::VirtualSelectedParentChainChangedNotification(_) => true,
            Payload::FinalityConflictNotification(_) => true,
            Payload::FinalityConflictResolvedNotification(_) => true,
            Payload::UtxosChangedNotification(_) => true,
            Payload::VirtualSelectedParentBlueScoreChangedNotification(_) => true,
            Payload::VirtualDaaScoreChangedNotification(_) => true,
            Payload::PruningPointUtxoSetOverrideNotification(_) => true,
            Payload::NewBlockTemplateNotification(_) => true,
            _ => false,
        }
    }
}
