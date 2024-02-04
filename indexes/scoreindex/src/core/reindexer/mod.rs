use kaspa_consensus_notify::notification::VirtualChainChangedNotification;

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
                .flat_map(|acceptance_data| {
                    acceptance_data.mergesets.iter().map(|mergeset| {
                        AcceptingBlueScoreHashPair::new(acceptance_data.accepting_blue_score, mergeset.block_hash)
                    })
                })
                .collect(),
            None,
        );
        Self { accepting_blue_score_changes }
    }
}
