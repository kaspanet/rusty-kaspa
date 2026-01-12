use std::time::Duration;

use super::IbdFlow;
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensusmanager::ConsensusProxy;
use kaspa_core::{debug, warn};
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    common::{ProtocolError, DEFAULT_TIMEOUT},
    dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, RequestIbdChainBlockLocatorMessage},
};

pub struct ChainNegotiationOutput {
    // Note: previous version peers (especially golang nodes) might return the headers selected tip here. Nonetheless
    // we name here following the currently implemented logic by which the syncer returns the virtual selected parent
    // chain on block locator queries
    pub syncer_virtual_selected_parent: Hash,
    pub highest_known_syncer_chain_hash: Option<Hash>,
    pub syncer_pruning_point: Hash,
}

impl IbdFlow {
    pub(super) async fn negotiate_missing_syncer_chain_segment(
        &mut self,
        consensus: &ConsensusProxy,
    ) -> Result<ChainNegotiationOutput, ProtocolError> {
        /*
            Algorithm:
                Request full selected chain block locator from syncer
                Find the highest block which we know
                Repeat the locator step over the new range until finding max(past(syncee) \cap chain(syncer))
        */

        // None hashes indicate that the full chain is queried
        let mut locator_hashes = self.get_syncer_chain_block_locator(None, None, DEFAULT_TIMEOUT).await?;
        if locator_hashes.is_empty() {
            return Err(ProtocolError::Other("Expecting initial syncer chain block locator to contain at least one element"));
        }
        let mut syncer_pruning_point = *locator_hashes.last().unwrap();

        debug!(
            "IBD chain negotiation with peer {} started and received {} hashes ({}, {})",
            self.router,
            locator_hashes.len(),
            locator_hashes[0],
            locator_hashes.last().unwrap()
        );

        let mut syncer_virtual_selected_parent = locator_hashes[0]; // Syncer sink (virtual selected parent)
        let highest_known_syncer_chain_hash: Option<Hash>;
        let mut negotiation_restart_counter = 0;
        let mut negotiation_zoom_counts = 0;
        let mut initial_locator_len = locator_hashes.len();
        loop {
            let mut lowest_unknown_syncer_chain_hash: Option<Hash> = None;
            let mut current_highest_known_syncer_chain_hash: Option<Hash> = None;
            for &syncer_chain_hash in locator_hashes.iter() {
                match consensus.async_get_block_status(syncer_chain_hash).await {
                    None => {
                        // Log the unknown block and continue to the next iteration
                        lowest_unknown_syncer_chain_hash = Some(syncer_chain_hash);
                    }
                    Some(BlockStatus::StatusInvalid) => {
                        return Err(ProtocolError::OtherOwned(format!("sent invalid chain block {}", syncer_chain_hash)));
                    }
                    Some(_) => {
                        current_highest_known_syncer_chain_hash = Some(syncer_chain_hash);
                        break;
                    }
                }
            }
            // No unknown blocks, break. Note this can only happen in the first iteration
            if lowest_unknown_syncer_chain_hash.is_none() {
                highest_known_syncer_chain_hash = current_highest_known_syncer_chain_hash;
                break;
            }
            // No shared block, break
            if current_highest_known_syncer_chain_hash.is_none() {
                highest_known_syncer_chain_hash = None;
                break;
            }
            // No point in zooming further
            if locator_hashes.len() == 1 {
                highest_known_syncer_chain_hash = current_highest_known_syncer_chain_hash;
                break;
            }
            // Zoom in
            locator_hashes = self
                .get_syncer_chain_block_locator(
                    current_highest_known_syncer_chain_hash,
                    lowest_unknown_syncer_chain_hash, // Note: both passed hashes are some
                    Duration::from_secs(10),          // We use a short timeout here to prevent a long spam negotiation
                )
                .await?;
            if !locator_hashes.is_empty() {
                if locator_hashes.first().copied() != lowest_unknown_syncer_chain_hash
                    || locator_hashes.last().copied() != current_highest_known_syncer_chain_hash
                {
                    return Err(ProtocolError::Other("Expecting the high and low hashes to match the locator bounds"));
                }
                negotiation_zoom_counts += 1;
                debug!(
                    "IBD chain negotiation with peer {} zoomed in ({}) and received {} hashes ({}, {})",
                    self.router,
                    negotiation_zoom_counts,
                    locator_hashes.len(),
                    locator_hashes[0],
                    locator_hashes.last().unwrap()
                );

                if locator_hashes.len() == 2 {
                    // We found our search target
                    highest_known_syncer_chain_hash = current_highest_known_syncer_chain_hash;
                    break;
                }

                if negotiation_zoom_counts > initial_locator_len * 2 {
                    // Since the zoom-in always queries two consecutive entries in the previous locator, it is
                    // expected to decrease in size at least every two iterations
                    return Err(ProtocolError::OtherOwned(format!(
                        "IBD chain negotiation: Number of zoom-in steps {} exceeded the upper bound of 2*{}",
                        negotiation_zoom_counts, initial_locator_len
                    )));
                }
            } else {
                // Empty locator signals a restart due to chain changes
                negotiation_zoom_counts = 0;
                negotiation_restart_counter += 1;
                if negotiation_restart_counter > 32 {
                    return Err(ProtocolError::OtherOwned(format!(
                        "IBD chain negotiation with syncer {} exceeded restart limit {}",
                        self.router, negotiation_restart_counter
                    )));
                }
                if negotiation_restart_counter > self.ctx.config.bps() {
                    // bps is just an intuitive threshold here
                    warn!("IBD chain negotiation with syncer {} restarted {} times", self.router, negotiation_restart_counter);
                } else {
                    debug!("IBD chain negotiation with syncer {} restarted {} times", self.router, negotiation_restart_counter);
                }

                // An empty locator signals that the syncer chain was modified and no longer contains one of
                // the queried hashes, so we restart the search. We use a shorter timeout here to avoid a timeout attack
                locator_hashes = self.get_syncer_chain_block_locator(None, None, Duration::from_secs(10)).await?;
                if locator_hashes.is_empty() {
                    return Err(ProtocolError::Other("Expecting initial syncer chain block locator to contain at least one element"));
                }

                debug!(
                    "IBD chain negotiation with peer {} restarted ({}) and received {} hashes ({}, {})",
                    self.router,
                    negotiation_restart_counter,
                    locator_hashes.len(),
                    locator_hashes[0],
                    locator_hashes.last().unwrap()
                );

                initial_locator_len = locator_hashes.len();
                // Reset syncer's virtual selected parent
                syncer_virtual_selected_parent = locator_hashes[0];
                syncer_pruning_point = *locator_hashes.last().unwrap();
            }
        }

        debug!("Found highest known syncer chain block {:?} from peer {}", highest_known_syncer_chain_hash, self.router);
        Ok(ChainNegotiationOutput { syncer_virtual_selected_parent, highest_known_syncer_chain_hash, syncer_pruning_point })
    }

    async fn get_syncer_chain_block_locator(
        &mut self,
        low: Option<Hash>,
        high: Option<Hash>,
        timeout: Duration,
    ) -> Result<Vec<Hash>, ProtocolError> {
        self.router
            .enqueue(make_message!(
                Payload::RequestIbdChainBlockLocator,
                RequestIbdChainBlockLocatorMessage { low_hash: low.map(|h| h.into()), high_hash: high.map(|h| h.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdChainBlockLocator, timeout)?;
        if msg.block_locator_hashes.len() > 64 {
            return Err(ProtocolError::Other(
                "Got block locator of size > 64 while expecting
 locator to have size which is logarithmic in DAG size (which should never exceed 2^64)",
            ));
        }
        Ok(msg.try_into()?)
    }
}
