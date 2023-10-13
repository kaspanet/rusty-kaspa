use kaspa_notify::{scope::Scope, subscription::Command};

use crate::protowire::{
    kaspad_request, kaspad_response, KaspadRequest, KaspadResponse, NotifyBlockAddedRequestMessage,
    NotifyFinalityConflictRequestMessage, NotifyNewBlockTemplateRequestMessage, NotifyPruningPointUtxoSetOverrideRequestMessage,
    NotifySinkBlueScoreChangedRequestMessage, NotifySyncStateChangedRequestMessage, NotifyUtxosChangedRequestMessage,
    NotifyVirtualChainChangedRequestMessage, NotifyVirtualDaaScoreChangedRequestMessage,
};

impl KaspadRequest {
    pub fn from_notification_type(scope: &Scope, command: Command) -> Self {
        KaspadRequest { id: 0, payload: Some(kaspad_request::Payload::from_notification_type(scope, command)) }
    }

    pub fn is_subscription(&self) -> bool {
        self.payload.as_ref().is_some_and(|x| x.is_subscription())
    }
}

impl kaspad_request::Payload {
    pub fn from_notification_type(scope: &Scope, command: Command) -> Self {
        match scope {
            Scope::BlockAdded(_) => {
                kaspad_request::Payload::NotifyBlockAddedRequest(NotifyBlockAddedRequestMessage { command: command.into() })
            }
            Scope::NewBlockTemplate(_) => {
                kaspad_request::Payload::NotifyNewBlockTemplateRequest(NotifyNewBlockTemplateRequestMessage {
                    command: command.into(),
                })
            }

            Scope::VirtualChainChanged(ref scope) => {
                kaspad_request::Payload::NotifyVirtualChainChangedRequest(NotifyVirtualChainChangedRequestMessage {
                    command: command.into(),
                    include_accepted_transaction_ids: scope.include_accepted_transaction_ids,
                })
            }
            Scope::FinalityConflict(_) => {
                kaspad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                    command: command.into(),
                })
            }
            Scope::FinalityConflictResolved(_) => {
                kaspad_request::Payload::NotifyFinalityConflictRequest(NotifyFinalityConflictRequestMessage {
                    command: command.into(),
                })
            }
            Scope::UtxosChanged(ref scope) => kaspad_request::Payload::NotifyUtxosChangedRequest(NotifyUtxosChangedRequestMessage {
                addresses: scope.addresses.iter().map(|x| x.into()).collect::<Vec<String>>(),
                command: command.into(),
            }),
            Scope::SinkBlueScoreChanged(_) => {
                kaspad_request::Payload::NotifySinkBlueScoreChangedRequest(NotifySinkBlueScoreChangedRequestMessage {
                    command: command.into(),
                })
            }
            Scope::VirtualDaaScoreChanged(_) => {
                kaspad_request::Payload::NotifyVirtualDaaScoreChangedRequest(NotifyVirtualDaaScoreChangedRequestMessage {
                    command: command.into(),
                })
            }
            Scope::PruningPointUtxoSetOverride(_) => {
                kaspad_request::Payload::NotifyPruningPointUtxoSetOverrideRequest(NotifyPruningPointUtxoSetOverrideRequestMessage {
                    command: command.into(),
                })
            }
            Scope::SyncStateChanged(_) => {
                kaspad_request::Payload::NotifySyncStateChangedRequest(NotifySyncStateChangedRequestMessage {
                    command: command.into(),
                })
            }
        }
    }

    pub fn is_subscription(&self) -> bool {
        use crate::protowire::kaspad_request::Payload;
        matches!(
            self,
            Payload::NotifyBlockAddedRequest(_)
                | Payload::NotifyVirtualChainChangedRequest(_)
                | Payload::NotifyFinalityConflictRequest(_)
                | Payload::NotifyUtxosChangedRequest(_)
                | Payload::NotifySinkBlueScoreChangedRequest(_)
                | Payload::NotifyVirtualDaaScoreChangedRequest(_)
                | Payload::NotifyPruningPointUtxoSetOverrideRequest(_)
                | Payload::NotifyNewBlockTemplateRequest(_)
                | Payload::StopNotifyingUtxosChangedRequest(_)
                | Payload::StopNotifyingPruningPointUtxoSetOverrideRequest(_)
                | Payload::NotifySyncStateChangedRequest(_)
        )
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

impl kaspad_response::Payload {
    pub fn is_notification(&self) -> bool {
        use crate::protowire::kaspad_response::Payload;
        matches!(
            self,
            Payload::BlockAddedNotification(_)
                | Payload::VirtualChainChangedNotification(_)
                | Payload::FinalityConflictNotification(_)
                | Payload::FinalityConflictResolvedNotification(_)
                | Payload::UtxosChangedNotification(_)
                | Payload::SinkBlueScoreChangedNotification(_)
                | Payload::VirtualDaaScoreChangedNotification(_)
                | Payload::PruningPointUtxoSetOverrideNotification(_)
                | Payload::NewBlockTemplateNotification(_)
                | Payload::SyncStateChangedNotification(_)
        )
    }
}
