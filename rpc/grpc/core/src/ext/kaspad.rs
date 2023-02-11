use kaspa_rpc_core::{api::ops::SubscribeCommand, notify::scope::Scope};

use crate::protowire::{
    kaspad_request, kaspad_response, KaspadRequest, KaspadResponse, NotifyBlockAddedRequestMessage,
    NotifyFinalityConflictRequestMessage, NotifyNewBlockTemplateRequestMessage, NotifyPruningPointUtxoSetOverrideRequestMessage,
    NotifyUtxosChangedRequestMessage, NotifyVirtualDaaScoreChangedRequestMessage,
    NotifyVirtualSelectedParentBlueScoreChangedRequestMessage, NotifyVirtualSelectedParentChainChangedRequestMessage,
};

impl KaspadRequest {
    pub fn from_notification_type(notification_type: &Scope, command: SubscribeCommand) -> Self {
        KaspadRequest { id: 0, payload: Some(kaspad_request::Payload::from_notification_type(notification_type, command)) }
    }
}

impl kaspad_request::Payload {
    pub fn from_notification_type(notification_type: &Scope, command: SubscribeCommand) -> Self {
        match notification_type {
            Scope::BlockAdded => {
                kaspad_request::Payload::NotifyBlockAddedRequest(NotifyBlockAddedRequestMessage { command: command.into() })
            }
            Scope::NewBlockTemplate => kaspad_request::Payload::NotifyNewBlockTemplateRequest(NotifyNewBlockTemplateRequestMessage {
                command: command.into(),
            }),

            Scope::VirtualSelectedParentChainChanged(ref scope) => {
                kaspad_request::Payload::NotifyVirtualSelectedParentChainChangedRequest(
                    NotifyVirtualSelectedParentChainChangedRequestMessage {
                        command: command.into(),
                        include_accepted_transaction_ids: scope.include_accepted_transaction_ids,
                    },
                )
            }
            Scope::FinalityConflict => kaspad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                command: command.into(),
            }),
            Scope::FinalityConflictResolved => {
                kaspad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                    command: command.into(),
                })
            }
            Scope::UtxosChanged(ref scope) => kaspad_request::Payload::NotifyUtxosChangedRequest(NotifyUtxosChangedRequestMessage {
                addresses: scope.addresses.iter().map(|x| x.into()).collect::<Vec<String>>(),
                command: command.into(),
            }),
            Scope::VirtualSelectedParentBlueScoreChanged => {
                kaspad_request::Payload::NotifyVirtualSelectedParentBlueScoreChangedRequest(
                    NotifyVirtualSelectedParentBlueScoreChangedRequestMessage { command: command.into() },
                )
            }
            Scope::VirtualDaaScoreChanged => {
                kaspad_request::Payload::NotifyVirtualDaaScoreChangedRequest(NotifyVirtualDaaScoreChangedRequestMessage {
                    command: command.into(),
                })
            }
            Scope::PruningPointUtxoSetOverride => {
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
