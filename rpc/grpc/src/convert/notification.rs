use std::str::FromStr;

use crate::protowire::{
    kaspad_response::Payload, BlockAddedNotificationMessage, KaspadResponse, NewBlockTemplateNotificationMessage, RpcNotifyCommand,
};
use crate::protowire::{
    FinalityConflictNotificationMessage, FinalityConflictResolvedNotificationMessage, NotifyPruningPointUtxoSetOverrideRequestMessage,
    NotifyPruningPointUtxoSetOverrideResponseMessage, NotifyUtxosChangedRequestMessage, NotifyUtxosChangedResponseMessage,
    PruningPointUtxoSetOverrideNotificationMessage, StopNotifyingPruningPointUtxoSetOverrideRequestMessage,
    StopNotifyingPruningPointUtxoSetOverrideResponseMessage, StopNotifyingUtxosChangedRequestMessage,
    StopNotifyingUtxosChangedResponseMessage, UtxosChangedNotificationMessage, VirtualDaaScoreChangedNotificationMessage,
    VirtualSelectedParentBlueScoreChangedNotificationMessage, VirtualSelectedParentChainChangedNotificationMessage,
};
use crate::{from, try_from};
use rpc_core::api::ops::SubscribeCommand;
use rpc_core::{Notification, RpcError, RpcHash};

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &rpc_core::Notification, KaspadResponse, { Self { id: 0, payload: Some(item.into()) } });

from!(item: &rpc_core::Notification, Payload, {
    match item {
        Notification::BlockAdded(ref notification) => Payload::BlockAddedNotification(notification.into()),
        Notification::NewBlockTemplate(ref notification) => Payload::NewBlockTemplateNotification(notification.into()),
        Notification::VirtualSelectedParentChainChanged(ref notification) => {
            Payload::VirtualSelectedParentChainChangedNotification(notification.into())
        }
        Notification::FinalityConflict(ref notification) => Payload::FinalityConflictNotification(notification.into()),
        Notification::FinalityConflictResolved(ref notification) => Payload::FinalityConflictResolvedNotification(notification.into()),
        Notification::UtxosChanged(ref notification) => Payload::UtxosChangedNotification(notification.into()),
        Notification::VirtualSelectedParentBlueScoreChanged(ref notification) => {
            Payload::VirtualSelectedParentBlueScoreChangedNotification(notification.into())
        }
        Notification::VirtualDaaScoreChanged(ref notification) => Payload::VirtualDaaScoreChangedNotification(notification.into()),
        Notification::PruningPointUtxoSetOverride(ref notification) => {
            Payload::PruningPointUtxoSetOverrideNotification(notification.into())
        }
    }
});

from!(item: &rpc_core::BlockAddedNotification, BlockAddedNotificationMessage, { Self { block: Some((&item.block).into()) } });

from!(&rpc_core::NewBlockTemplateNotification, NewBlockTemplateNotificationMessage);

from!(item: &rpc_core::VirtualSelectedParentChainChangedNotification, VirtualSelectedParentChainChangedNotificationMessage, {
    Self {
        removed_chain_block_hashes: item.removed_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| x.to_string()).collect(),
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.into()).collect(),
    }
});

from!(item: &rpc_core::FinalityConflictNotification, FinalityConflictNotificationMessage, {
    Self { violating_block_hash: item.violating_block_hash.to_string() }
});

from!(item: &rpc_core::FinalityConflictResolvedNotification, FinalityConflictResolvedNotificationMessage, {
    Self { finality_block_hash: item.finality_block_hash.to_string() }
});

from!(item: &rpc_core::UtxosChangedNotification, UtxosChangedNotificationMessage, {
    Self {
        added: item.added.iter().map(|x| x.into()).collect::<Vec<_>>(),
        removed: item.removed.iter().map(|x| x.into()).collect::<Vec<_>>(),
    }
});

from!(item: &rpc_core::VirtualSelectedParentBlueScoreChangedNotification, VirtualSelectedParentBlueScoreChangedNotificationMessage, {
    Self { virtual_selected_parent_blue_score: item.virtual_selected_parent_blue_score }
});

from!(item: &rpc_core::VirtualDaaScoreChangedNotification, VirtualDaaScoreChangedNotificationMessage, {
    Self { virtual_daa_score: item.virtual_daa_score }
});

from!(&rpc_core::PruningPointUtxoSetOverrideNotification, PruningPointUtxoSetOverrideNotificationMessage);

from!(item: SubscribeCommand, RpcNotifyCommand, {
    match item {
        SubscribeCommand::Start => RpcNotifyCommand::NotifyStart,
        SubscribeCommand::Stop => RpcNotifyCommand::NotifyStop,
    }
});

from!(item: &StopNotifyingUtxosChangedRequestMessage, NotifyUtxosChangedRequestMessage, {
    Self { addresses: item.addresses.clone(), command: SubscribeCommand::Stop.into() }
});

from!(_item: &StopNotifyingPruningPointUtxoSetOverrideRequestMessage, NotifyPruningPointUtxoSetOverrideRequestMessage, {
    Self { command: SubscribeCommand::Stop.into() }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &KaspadResponse, rpc_core::Notification, {
    item.payload
        .as_ref()
        .ok_or_else(|| RpcError::MissingRpcFieldError("KaspadResponse".to_string(), "payload".to_string()))?
        .try_into()?
});

try_from!(item: &Payload, rpc_core::Notification, {
    match item {
        Payload::BlockAddedNotification(ref notification) => Notification::BlockAdded(notification.try_into()?),
        Payload::NewBlockTemplateNotification(ref notification) => Notification::NewBlockTemplate(notification.try_into()?),
        Payload::VirtualSelectedParentChainChangedNotification(ref notification) => {
            Notification::VirtualSelectedParentChainChanged(notification.try_into()?)
        }
        Payload::FinalityConflictNotification(ref notification) => Notification::FinalityConflict(notification.try_into()?),
        Payload::FinalityConflictResolvedNotification(ref notification) => {
            Notification::FinalityConflictResolved(notification.try_into()?)
        }
        Payload::UtxosChangedNotification(ref notification) => Notification::UtxosChanged(notification.try_into()?),
        Payload::VirtualSelectedParentBlueScoreChangedNotification(ref notification) => {
            Notification::VirtualSelectedParentBlueScoreChanged(notification.try_into()?)
        }
        Payload::VirtualDaaScoreChangedNotification(ref notification) => {
            Notification::VirtualDaaScoreChanged(notification.try_into()?)
        }
        Payload::PruningPointUtxoSetOverrideNotification(ref notification) => {
            Notification::PruningPointUtxoSetOverride(notification.try_into()?)
        }
        _ => Err(RpcError::UnsupportedFeature)?,
    }
});

try_from!(item: &BlockAddedNotificationMessage, rpc_core::BlockAddedNotification, {
    Self {
        block: item
            .block
            .as_ref()
            .ok_or_else(|| RpcError::MissingRpcFieldError("BlockAddedNotificationMessage".to_string(), "block".to_string()))?
            .try_into()?,
    }
});

try_from!(&NewBlockTemplateNotificationMessage, rpc_core::NewBlockTemplateNotification);

try_from!(item: &VirtualSelectedParentChainChangedNotificationMessage, rpc_core::VirtualSelectedParentChainChangedNotification, {
    Self {
        removed_chain_block_hashes: item
            .removed_chain_block_hashes
            .iter()
            .map(|x| RpcHash::from_str(x))
            .collect::<Result<Vec<_>, _>>()?,
        added_chain_block_hashes: item.added_chain_block_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?,
        accepted_transaction_ids: item.accepted_transaction_ids.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(item: &FinalityConflictNotificationMessage, rpc_core::FinalityConflictNotification, {
    Self { violating_block_hash: RpcHash::from_str(&item.violating_block_hash)? }
});

try_from!(item: &FinalityConflictResolvedNotificationMessage, rpc_core::FinalityConflictResolvedNotification, {
    Self { finality_block_hash: RpcHash::from_str(&item.finality_block_hash)? }
});

try_from!(item: &UtxosChangedNotificationMessage, rpc_core::UtxosChangedNotification, {
    Self {
        added: item.added.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
        removed: item.removed.iter().map(|x| x.try_into()).collect::<Result<Vec<_>, _>>()?,
    }
});

try_from!(
    item: &VirtualSelectedParentBlueScoreChangedNotificationMessage,
    rpc_core::VirtualSelectedParentBlueScoreChangedNotification,
    { Self { virtual_selected_parent_blue_score: item.virtual_selected_parent_blue_score } }
);

try_from!(item: &VirtualDaaScoreChangedNotificationMessage, rpc_core::VirtualDaaScoreChangedNotification, {
    Self { virtual_daa_score: item.virtual_daa_score }
});

try_from!(&PruningPointUtxoSetOverrideNotificationMessage, rpc_core::PruningPointUtxoSetOverrideNotification);

from!(item: RpcNotifyCommand, SubscribeCommand, {
    match item {
        RpcNotifyCommand::NotifyStart => SubscribeCommand::Start,
        RpcNotifyCommand::NotifyStop => SubscribeCommand::Stop,
    }
});

from!(item: NotifyUtxosChangedResponseMessage, StopNotifyingUtxosChangedResponseMessage, { Self { error: item.error } });

from!(item: NotifyPruningPointUtxoSetOverrideResponseMessage, StopNotifyingPruningPointUtxoSetOverrideResponseMessage, {
    Self { error: item.error }
});
