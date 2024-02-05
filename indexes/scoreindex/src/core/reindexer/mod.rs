use kaspa_consensus_notify::notification::{ChainAcceptanceDataPrunedNotification, VirtualChainChangedNotification};

use crate::{AcceptingBlueScoreDiff, AcceptingBlueScoreHashPair};

pub struct ScoreIndexReindexer {
    pub accepting_blue_score_changes: AcceptingBlueScoreDiff,
}

impl From<VirtualChainChangedNotification> for ScoreIndexReindexer {
    fn from(notification: VirtualChainChangedNotification) -> Self {
        let accepting_blue_score_changes = AcceptingBlueScoreDiff::new(
            notification
                .removed_chain_blocks_acceptance_data
                .iter()
                .map(|acceptance_data| acceptance_data.accepting_blue_score)
                .collect(),
            notification
                .added_chain_blocks_acceptance_data
                .iter()
                .zip(notification.added_chain_block_hashes.iter())
                .map(|(acceptance_data, hash)| AcceptingBlueScoreHashPair::new(acceptance_data.accepting_blue_score, *hash))
                .collect(),
        );
        Self { accepting_blue_score_changes }
    }
}

impl From<ChainAcceptanceDataPrunedNotification> for ScoreIndexReindexer {
    fn from(notification: ChainAcceptanceDataPrunedNotification) -> Self {
        let accepting_blue_score_changes =
            AcceptingBlueScoreDiff::new(vec![notification.mergeset_block_acceptance_data_pruned.accepting_blue_score], vec![]);
        Self { accepting_blue_score_changes }
    }
}
