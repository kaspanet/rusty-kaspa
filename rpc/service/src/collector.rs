use crate::converter::consensus::ConsensusConverter;
use kaspa_index_core::notification::Notification as IndexNotification;
use kaspa_notify::{collector::CollectorFrom, converter::ConverterFrom};
use kaspa_rpc_core::Notification;

pub(crate) type CollectorFromConsensus = CollectorFrom<ConsensusConverter>;

pub(crate) type IndexConverter = ConverterFrom<IndexNotification, Notification>;
pub(crate) type CollectorFromIndex = CollectorFrom<IndexConverter>;
