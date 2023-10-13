use crate::{
    from,
    protowire::{
        kaspad_response::Payload, sync_state_changed_notification_message, sync_state_changed_notification_message::SyncState,
        BlockAddedNotificationMessage, BlocksState, FinalityConflictNotificationMessage, FinalityConflictResolvedNotificationMessage,
        HeadersState, KaspadResponse, NewBlockTemplateNotificationMessage, NotifyPruningPointUtxoSetOverrideRequestMessage,
        NotifyPruningPointUtxoSetOverrideResponseMessage, NotifyUtxosChangedRequestMessage, NotifyUtxosChangedResponseMessage,
        ProofState, PruningPointUtxoSetOverrideNotificationMessage, RpcNotifyCommand, SinkBlueScoreChangedNotificationMessage,
        StopNotifyingPruningPointUtxoSetOverrideRequestMessage, StopNotifyingPruningPointUtxoSetOverrideResponseMessage,
        StopNotifyingUtxosChangedRequestMessage, StopNotifyingUtxosChangedResponseMessage, SyncStateChangedNotificationMessage,
        TrustSyncState, UtxoSyncState, UtxosChangedNotificationMessage, VirtualChainChangedNotificationMessage,
        VirtualDaaScoreChangedNotificationMessage,
    },
    try_from,
};
use kaspa_notify::subscription::Command;
use kaspa_rpc_core::{Notification, RpcError, RpcHash, SyncStateChangedNotification};
use std::str::FromStr;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::Notification, KaspadResponse, { Self { id: 0, payload: Some(item.into()) } });

from!(item: &kaspa_rpc_core::Notification, Payload, {
    match item {
        Notification::BlockAdded(notification) => Payload::BlockAddedNotification(notification.into()),
        Notification::NewBlockTemplate(notification) => Payload::NewBlockTemplateNotification(notification.into()),
        Notification::VirtualChainChanged(notification) => Payload::VirtualChainChangedNotification(notification.into()),
        Notification::FinalityConflict(notification) => Payload::FinalityConflictNotification(notification.into()),
        Notification::FinalityConflictResolved(notification) => Payload::FinalityConflictResolvedNotification(notification.into()),
        Notification::UtxosChanged(notification) => Payload::UtxosChangedNotification(notification.into()),
        Notification::SinkBlueScoreChanged(notification) => Payload::SinkBlueScoreChangedNotification(notification.into()),
        Notification::VirtualDaaScoreChanged(notification) => Payload::VirtualDaaScoreChangedNotification(notification.into()),
        Notification::PruningPointUtxoSetOverride(notification) => {
            Payload::PruningPointUtxoSetOverrideNotification(notification.into())
        },
        Notification::SyncStateChanged(notification) => Payload::SyncStateChangedNotification(notification.into()),
    }
});

from!(item: &kaspa_rpc_core::BlockAddedNotification, BlockAddedNotificationMessage, { Self { block: Some((&*item.block).into()) } });

from!(&kaspa_rpc_core::NewBlockTemplateNotification, NewBlockTemplateNotificationMessage);

from!(item: &kaspa_rpc_core::VirtualChainChangedNotification, VirtualChainChangedNotificationMessage, {
    Self {
        removed_chain_block_hashes: item.removed_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.into()).collect(),
    }
});

from!(item: &kaspa_rpc_core::FinalityConflictNotification, FinalityConflictNotificationMessage, {
    Self { violating_block_hash: item.violating_block_hash.to_string() }
});

from!(item: &kaspa_rpc_core::FinalityConflictResolvedNotification, FinalityConflictResolvedNotificationMessage, {
    Self { finality_block_hash: item.finality_block_hash.to_string() }
});

from!(item: &kaspa_rpc_core::UtxosChangedNotification, UtxosChangedNotificationMessage, {
    Self {
        added: item.added.iter().map(|x| x.into()).collect::<Vec<_>>(),
        removed: item.removed.iter().map(|x| x.into()).collect::<Vec<_>>(),
    }
});

from!(item: &kaspa_rpc_core::SinkBlueScoreChangedNotification, SinkBlueScoreChangedNotificationMessage, {
    Self { sink_blue_score: item.sink_blue_score }
});

from!(item: &kaspa_rpc_core::VirtualDaaScoreChangedNotification, VirtualDaaScoreChangedNotificationMessage, {
    Self { virtual_daa_score: item.virtual_daa_score }
});

from!(item: &kaspa_rpc_core::SyncStateChangedNotification, SyncStateChangedNotificationMessage, {
    match item {
            SyncStateChangedNotification::Proof { current, max } => SyncStateChangedNotificationMessage {
                sync_state: Some(sync_state_changed_notification_message::SyncState::Proof(crate::protowire::ProofState {
                    current: *current as u32,
                    max: *max as u32,
                })),
            },
            SyncStateChangedNotification::Headers { headers, progress} => SyncStateChangedNotificationMessage {
                sync_state: Some(sync_state_changed_notification_message::SyncState::Headers(crate::protowire::HeadersState {
                    headers: *headers,
                    progress: *progress
                }))
            },
            SyncStateChangedNotification::Blocks { blocks, progress} => SyncStateChangedNotificationMessage {
                sync_state: Some(sync_state_changed_notification_message::SyncState::Blocks(crate::protowire::BlocksState {
                    blocks: *blocks,
                    progress: *progress
                }))
            },
            SyncStateChangedNotification::UtxoResync => SyncStateChangedNotificationMessage {
                sync_state: Some(sync_state_changed_notification_message::SyncState::UtxoResync(crate::protowire::UtxoResyncState{}))
            },
            SyncStateChangedNotification::UtxoSync { chunks, total} => SyncStateChangedNotificationMessage {
                sync_state: Some(sync_state_changed_notification_message::SyncState::UtxoSync(crate::protowire::UtxoSyncState {
                    chunks: *chunks,
                    total: *total
                }))
            },
            SyncStateChangedNotification::TrustSync { processed, total} => SyncStateChangedNotificationMessage {
                sync_state: Some(sync_state_changed_notification_message::SyncState::TrustSync(crate::protowire::TrustSyncState {
                    processed: *processed,
                    total: *total
                }))
            },
            SyncStateChangedNotification::Synced => SyncStateChangedNotificationMessage {
                sync_state: Some(sync_state_changed_notification_message::SyncState::Synced(crate::protowire::SyncedState{}))
            },
        }
});

from!(&kaspa_rpc_core::PruningPointUtxoSetOverrideNotification, PruningPointUtxoSetOverrideNotificationMessage);

from!(item: Command, RpcNotifyCommand, {
    match item {
        Command::Start => RpcNotifyCommand::NotifyStart,
        Command::Stop => RpcNotifyCommand::NotifyStop,
    }
});

from!(item: &StopNotifyingUtxosChangedRequestMessage, NotifyUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.clone(), command: Command::Stop.into() }
});

from!(_item: &StopNotifyingPruningPointUtxoSetOverrideRequestMessage, NotifyPruningPointUtxoSetOverrideRequestMessage, {
    Self { command: Command::Stop.into() }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &KaspadResponse, kaspa_rpc_core::Notification, {
    item.payload
        .as_ref()
        .ok_or_else(|| RpcError::MissingRpcFieldError("KaspadResponse".to_string(), "payload".to_string()))?
        .try_into()?
});

try_from!(item: &Payload, kaspa_rpc_core::Notification, {
    match item {
        Payload::BlockAddedNotification(ref notification) => Notification::BlockAdded(notification.try_into()?),
        Payload::NewBlockTemplateNotification(ref notification) => Notification::NewBlockTemplate(notification.try_into()?),
        Payload::VirtualChainChangedNotification(ref notification) => Notification::VirtualChainChanged(notification.try_into()?),
        Payload::FinalityConflictNotification(ref notification) => Notification::FinalityConflict(notification.try_into()?),
        Payload::FinalityConflictResolvedNotification(ref notification) => {
            Notification::FinalityConflictResolved(notification.try_into()?)
        }
        Payload::UtxosChangedNotification(ref notification) => Notification::UtxosChanged(notification.try_into()?),
        Payload::SinkBlueScoreChangedNotification(ref notification) => Notification::SinkBlueScoreChanged(notification.try_into()?),
        Payload::VirtualDaaScoreChangedNotification(ref notification) => {
            Notification::VirtualDaaScoreChanged(notification.try_into()?)
        }
        Payload::PruningPointUtxoSetOverrideNotification(ref notification) => {
            Notification::PruningPointUtxoSetOverride(notification.try_into()?)
        }
        Payload::SyncStateChangedNotification(ref notification) => {
             Notification::SyncStateChanged(notification.try_into()?)
        }
        _ => Err(RpcError::UnsupportedFeature)?,
    }
});

try_from!(item: &BlockAddedNotificationMessage, kaspa_rpc_core::BlockAddedNotification, {
    Self {
        block: Arc::new(
            item.block
                .as_ref()
                .ok_or_else(|| RpcError::MissingRpcFieldError("BlockAddedNotificationMessage".to_string(), "block".to_string()))?
                .try_into()?,
        ),
    }
});

try_from!(&NewBlockTemplateNotificationMessage, kaspa_rpc_core::NewBlockTemplateNotification);

try_from!(item: &VirtualChainChangedNotificationMessage, kaspa_rpc_core::VirtualChainChangedNotification, {
    Self {
        removed_chain_block_hashes: Arc::new(
            item.removed_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        ),
        added_chain_block_hashes: Arc::new(
            item.added_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        ),
        accepted_transaction_ids: Arc::new(item.accepted_transaction_ids.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?),
    }
});

try_from!(item: &FinalityConflictNotificationMessage, kaspa_rpc_core::FinalityConflictNotification, {
    Self { violating_block_hash: RpcHash::from_str(&item.violating_block_hash)? }
});

try_from!(item: &FinalityConflictResolvedNotificationMessage, kaspa_rpc_core::FinalityConflictResolvedNotification, {
    Self { finality_block_hash: RpcHash::from_str(&item.finality_block_hash)? }
});

try_from!(item: &UtxosChangedNotificationMessage, kaspa_rpc_core::UtxosChangedNotification, {
    Self {
        added: Arc::new(item.added.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?),
        removed: Arc::new(item.removed.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?),
    }
});

try_from!(item: &SinkBlueScoreChangedNotificationMessage, kaspa_rpc_core::SinkBlueScoreChangedNotification, {
    Self { sink_blue_score: item.sink_blue_score }
});

try_from!(item: &VirtualDaaScoreChangedNotificationMessage, kaspa_rpc_core::VirtualDaaScoreChangedNotification, {
    Self { virtual_daa_score: item.virtual_daa_score }
});

try_from!(&PruningPointUtxoSetOverrideNotificationMessage, kaspa_rpc_core::PruningPointUtxoSetOverrideNotification);

from!(item: RpcNotifyCommand, Command, {
    match item {
        RpcNotifyCommand::NotifyStart => Command::Start,
        RpcNotifyCommand::NotifyStop => Command::Stop,
    }
});

from!(item: NotifyUtxosChangedResponseMessage, StopNotifyingUtxosChangedResponseMessage, { Self { error: item.error } });

from!(item: NotifyPruningPointUtxoSetOverrideResponseMessage, StopNotifyingPruningPointUtxoSetOverrideResponseMessage, {
    Self { error: item.error }
});

impl TryFrom<&SyncStateChangedNotificationMessage> for kaspa_rpc_core::SyncStateChangedNotification {
    type Error = RpcError;

    fn try_from(value: &SyncStateChangedNotificationMessage) -> Result<Self, Self::Error> {
        let sync_state = value
            .sync_state
            .as_ref()
            .ok_or(RpcError::MissingRpcFieldError(String::from("SyncStateChangedNotificationMessage"), String::from("syncState")))?;
        let notification = match sync_state {
            SyncState::Proof(ProofState { current, max }) => {
                kaspa_rpc_core::SyncStateChangedNotification::Proof { current: *current as u8, max: *max as u8 }
            }
            SyncState::Headers(HeadersState { headers, progress }) => {
                kaspa_rpc_core::SyncStateChangedNotification::Headers { headers: *headers, progress: *progress }
            }
            SyncState::Blocks(BlocksState { blocks, progress }) => {
                kaspa_rpc_core::SyncStateChangedNotification::Blocks { blocks: *blocks, progress: *progress }
            }
            SyncState::UtxoResync(_) => kaspa_rpc_core::SyncStateChangedNotification::UtxoResync,
            SyncState::UtxoSync(UtxoSyncState { chunks, total }) => {
                kaspa_rpc_core::SyncStateChangedNotification::UtxoSync { chunks: *chunks, total: *total }
            }
            SyncState::TrustSync(TrustSyncState { processed, total }) => {
                kaspa_rpc_core::SyncStateChangedNotification::TrustSync { processed: *processed, total: *total }
            }
            SyncState::Synced(_) => kaspa_rpc_core::SyncStateChangedNotification::Synced,
        };
        Ok(notification)
    }
}
