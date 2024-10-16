use super::{
    error::ReceiptsError, pchmr_store::{DbPchmrStore, PchmrStoreReader}, rep_parents_store::{DbRepParentsStore, RepParentsStoreReader}
};
use crate::model::stores::selected_chain:: SelectedChainStoreReader;
#[allow(unused_imports)]
use crate::{
    consensus::{
        services::{
            ConsensusServices, DbBlockDepthManager, DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbPruningPointManager,
            DbWindowManager,
        },
        storage::ConsensusStorage,
    },
    constants::BLOCK_VERSION,
    errors::RuleError,
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            acceptance_data::{AcceptanceDataStoreReader, DbAcceptanceDataStore},
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            daa::DbDaaStore,
            depth::{DbDepthStore, DepthStoreReader},
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{DbPruningStore, PruningStoreReader},
            pruning_utxoset::PruningUtxosetStores,
            reachability::{DbReachabilityStore, ReachabilityStoreReader},
            relations::{DbRelationsStore, RelationsStoreReader},
            selected_chain::{DbSelectedChainStore, SelectedChainStore},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            virtual_state::{LkgVirtualState, VirtualState, VirtualStateStoreReader, VirtualStores},
            DB,
        },
    },
    params::Params,
    pipeline::{
        deps_manager::VirtualStateProcessingMessage, pruning_processor::processor::PruningProcessingMessage, ProcessingCounters,
    },
    processes::{
        coinbase::CoinbaseManager,
        ghostdag::ordering::SortableBlock,
        transaction_validator::{errors::TxResult, transaction_validator_populated::TxValidationFlags, TransactionValidator},
        window::WindowManager,
    },
};
#[allow(unused_imports)]
use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    api::args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    block::{BlockTemplate, MutableBlock, TemplateBuildMode, TemplateTransactionSelector},
    blockstatus::BlockStatus::{StatusDisqualifiedFromChain, StatusUTXOValid},
    coinbase::MinerData,
    config::genesis::GenesisBlock,
    header::Header,
    merkle::calc_hash_merkle_root,
    pruning::PruningPointsList,
    tx::{MutableTransaction, Transaction},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
    BlockHashSet, ChainPath,
};
#[allow(unused_imports)]
use kaspa_consensus_notify::{
    notification::{
        NewBlockTemplateNotification, Notification, SinkBlueScoreChangedNotification, UtxosChangedNotification,
        VirtualChainChangedNotification, VirtualDaaScoreChangedNotification,
    },
    root::ConsensusNotificationRoot,
};
#[allow(unused_imports)]
use kaspa_consensusmanager::SessionLock;
#[allow(unused_imports)]
use kaspa_core::{debug, info, time::unix_now, trace, warn};
#[allow(unused_imports)]
use kaspa_database::prelude::{StoreError, StoreResultEmptyTuple, StoreResultExtensions};
#[allow(unused_imports)]
use kaspa_hashes::Hash;
use kaspa_merkle::{
    calc_merkle_root, create_merkle_witness_from_sorted, create_merkle_witness_from_unsorted, verify_merkle_witness, MerkleWitness
};
#[allow(unused_imports)]
use kaspa_muhash::MuHash;
#[allow(unused_imports)]
use kaspa_notify::{events::EventType, notifier::Notify};

// use super::errors::{PruningImportError, PruningImportResult};
#[allow(unused_imports)]
use crossbeam_channel::{Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
#[allow(unused_imports)]
use itertools::Itertools;
#[allow(unused_imports)]
use kaspa_consensus_core::tx::ValidatedTransaction;
#[allow(unused_imports)]
use kaspa_utils::binary_heap::BinaryHeapExtensions;
#[allow(unused_imports)]
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
#[allow(unused_imports)]
use rand::{seq::SliceRandom, Rng};

#[allow(unused_imports)]
use rayon::{
    prelude::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator},
    ThreadPool,
};
#[allow(unused_imports)]
use rocksdb::WriteBatch;
#[allow(unused_imports)]
use std::{
    cmp::min,
    collections::{BinaryHeap, HashMap, VecDeque},
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};
#[derive(Clone)]
pub struct MerkleProofsManager<T:SelectedChainStoreReader,U: ReachabilityStoreReader,V:HeaderStoreReader> {
    // Channels
    // receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
    // pruning_sender: CrossbeamSender<PruningProcessingMessage>,
    // pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // Thread pool
    // pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    // db: Arc<DB>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) max_block_parents: u8,
    pub(super) mergeset_size_limit: u64,
    pub(super) pruning_depth: u64,
    pub(super) posterity_depth: u64,
    pub(super) average_width: u8,

    // Stores
    // pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    // pub(super) ghostdag_primary_store: Arc<DbGhostdagStore>,
    headers_store: Arc<V>,
    selected_chain_store: Arc<RwLock<T>>,    // pub(super) daa_excluded_store: Arc<DbDaaStore>,
    // pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    // pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,
    // pub(super) depth_store: Arc<DbDepthStore>,
    pub(super) hash_to_pchmr_store: Arc<DbPchmrStore>,
    pub(super) rep_parents_store: Arc<DbRepParentsStore>,

    // // Utxo-related stores
    // pub(super) utxo_diffs_store: Arc<DbUtxoDiffsStore>,
    // pub(super) utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    // pub(super) virtual_stores: Arc<RwLock<VirtualStores>>,
    // pub(super) pruning_utxoset_stores: Arc<RwLock<PruningUtxosetStores>>,

    // /// The "last known good" virtual state. To be used by any logic which does not want to wait
    // /// for a possible virtual state write to complete but can rather settle with the last known state
    // pub lkg_virtual_state: LkgVirtualState,

    // // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super )reachability_service: MTReachabilityService<U>,    // pub(super) relations_service: MTRelationsService<DbRelationsStore>,
    pub(super) dag_traversal_manager: DbDagTraversalManager,
    // pub(super) window_manager: DbWindowManager,
    // pub(super) coinbase_manager: CoinbaseManager,
    // pub(super) transaction_validator: TransactionValidator,
    pub(super) pruning_point_manager: DbPruningPointManager,
    
    // pub(super) parents_manager: DbParentsManager,
    // pub(super) depth_manager: DbBlockDepthManager,

    // // Pruning lock
    // pruning_lock: SessionLock,

    // // Notifier
    // notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    // counters: Arc<ProcessingCounters>,

    // Storage mass hardfork DAA score
    pub(crate) storage_mass_activation_daa_score: u64,
}

impl <T:SelectedChainStoreReader,U: ReachabilityStoreReader,V:HeaderStoreReader> MerkleProofsManager<T,U,V,>  {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        // receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
        // pruning_sender: CrossbeamSender<PruningProcessingMessage>,
        // pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,
        // thread_pool: Arc<ThreadPool>,
        params: &Params,
        // db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        dag_traversal_manager:DbDagTraversalManager,
        pruning_point_manager: DbPruningPointManager,
        ghostdag_manager: DbGhostdagManager,

        reachability_service: MTReachabilityService<U>,
        headers_store: Arc<V>,
        selected_chain_store: Arc<RwLock<T>>,

        // pruning_lock: SessionLock,
        // notification_root: Arc<ConsensusNotificationRoot>,
        // counters: Arc<ProcessingCounters>,
    ) -> Self {
        Self {
            // receiver,
            // pruning_sender,
            // pruning_receiver,
            // thread_pool,
            genesis: params.genesis.clone(),
            max_block_parents: params.max_block_parents,
            mergeset_size_limit: params.mergeset_size_limit,
            pruning_depth: params.pruning_depth,
            posterity_depth: params.pruning_depth,
            average_width: 2, //hardcoded, should advise with others if this is the correct solution

            // db,
            // statuses_store: storage.statuses_store.clone(),
            headers_store: headers_store,
            // ghostdag_primary_store: storage.ghostdag_primary_store.clone(),
            // daa_excluded_store: storage.daa_excluded_store.clone(),
            // block_transactions_store: storage.block_transactions_store.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            // body_tips_store: storage.body_tips_store.clone(),
            // depth_store: storage.depth_store.clone(),
            selected_chain_store: selected_chain_store.clone(),
            // utxo_diffs_store: storage.utxo_diffs_store.clone(),
            // utxo_multisets_store: storage.utxo_multisets_store.clone(),
            acceptance_data_store: storage.acceptance_data_store.clone(),
            // virtual_stores: storage.virtual_stores.clone(),
            // pruning_utxoset_stores: storage.pruning_utxoset_stores.clone(),
            // lkg_virtual_state: storage.lkg_virtual_state.clone(),

            ghostdag_manager: ghostdag_manager.clone(),
            reachability_service,
            // relations_service: services.relations_service.clone(),
            dag_traversal_manager: dag_traversal_manager.clone(),
            // window_manager: services.window_manager.clone(),
            // coinbase_manager: services.coinbase_manager.clone(),
            // transaction_validator: services.transaction_validator.clone(),
            pruning_point_manager: pruning_point_manager.clone(),
            // parents_manager: services.parents_manager.clone(),
            // depth_manager: services.depth_manager.clone(),

            // pruning_lock,
            // notification_root,
            // counters,
            storage_mass_activation_daa_score: params.storage_mass_activation_daa_score,
            hash_to_pchmr_store: storage.hash_to_pchmr_store.clone(),
            rep_parents_store: storage.rep_parents_store.clone(),
        }
    }

    pub fn create_merkle_witness_for_tx(&self, tracked_tx_id: Hash, req_block_hash: Hash) -> Result<MerkleWitness,ReceiptsError> {
        // maybe better here to make it return result? rethink
        let mergeset_txs_manager = self.acceptance_data_store.get(req_block_hash); // I think this is incorrect
        let mergeset_txs_manager = mergeset_txs_manager?;
        let mut accepted_txs = vec!();

        for parent_acc_data in mergeset_txs_manager.iter() {
            let parent_acc_txs = parent_acc_data.accepted_transactions.iter().map(|tx| tx.transaction_id);

            for tx in parent_acc_txs {
                accepted_txs.push(tx);
            }
        }
        accepted_txs.sort();

        create_merkle_witness_from_sorted(accepted_txs.into_iter(), tracked_tx_id).map_err(|e|e.into())
    }
    pub fn verify_merkle_witness_for_tx(&self, witness: &MerkleWitness, tracked_tx_id: Hash, req_block_hash: Hash) -> bool {
        // arguably better here to make it return result? rethink
        let mergeset_txs_manager = self.acceptance_data_store.get(req_block_hash); // I think this is incorrect
        if mergeset_txs_manager.is_err() {
            return false;
        }
        let req_block_header = self.headers_store.get_header(req_block_hash).unwrap();
        let req_atmr = req_block_header.accepted_id_merkle_root;
        verify_merkle_witness(witness, tracked_tx_id, req_atmr)
    }
    fn representative_log_parents(&self, req_block_parent: Hash) -> Vec<Hash> {
        // function receives the selected parent of the relevant block
        //returns all 2^i deep 'selected' parents up to the posterity block not included
        let pre_posterity_hash = self.get_prev_posterity_block(req_block_parent).unwrap();
        let pre_posterity_bscore = self.headers_store.get_blue_score(pre_posterity_hash).unwrap();
        let mut representative_parents_list = vec!();
        /*  currently I store the representative parents of each block in memory to implement this efficiently,
        there likely are better ways, but we might change to a completely different method later on
        so currently not worth too much thought*/
        let mut i = 0;
        let mut curr_block = self.reachability_service.default_backward_chain_iterator(req_block_parent).next();
        while curr_block.is_some() && self.headers_store.get_blue_score(pre_posterity_hash).unwrap() > pre_posterity_bscore {
            representative_parents_list.push(curr_block.unwrap());
            curr_block = self.rep_parents_store.get_ith_rep_parent(curr_block.unwrap(), i);
            i += 1;
        }
        representative_parents_list
    }
    #[allow(clippy::let_and_return)]
    pub fn calc_pchmr_root(&self, selected_parent: Hash) -> Hash {
        // function receives the selected parent of the relevant block
        let representative_parents_list = self.representative_log_parents(selected_parent);
        let ret = calc_merkle_root(representative_parents_list.into_iter());

        ret
    }
    pub fn create_pchmr_witness(&self, leaf_block_hash: Hash, root_block_hash: Hash) -> Result<MerkleWitness,ReceiptsError> {
        // proof that a block belongs to the prhmr tree of another block
        let parent_of_root = self.reachability_service.get_chain_parent(root_block_hash);
        let log_sized_parents_list = self.representative_log_parents(parent_of_root);

        create_merkle_witness_from_unsorted(log_sized_parents_list.into_iter(), leaf_block_hash).map_err(|e|e.into())
    }
    pub fn verify_pchmr_witness(&self, witness: &MerkleWitness, leaf_block_hash: Hash, root_block_hash: Hash) -> bool {
        verify_merkle_witness(witness, leaf_block_hash, self.hash_to_pchmr_store.get(root_block_hash).unwrap())
    }
    pub fn create_pochm_proof(&self, req_block_hash: Hash) -> Result<Pochm,ReceiptsError> {
        //Assumes: requested block hash is on the selected chain
        //needs be relooked at
        let mut proof = vec!();
        let post_posterity_hash = self.get_post_posterity_block(req_block_hash)?;
        let req_block_index = self.selected_chain_store.read().get_by_hash(req_block_hash).unwrap();
        let (mut prev_hash, mut prev_index) =
            (post_posterity_hash, self.selected_chain_store.read().get_by_hash(post_posterity_hash).unwrap());
        let mut curr_index; //init value for compiler
        let mut diff = prev_index - req_block_index;
        let mut curr_hash; //init value for compiler
        while diff > 0 {
            curr_index = prev_index - (diff + 1).next_power_of_two() / 2; //highest power of two such that it does not pass below prev_index-req_block_index
            curr_hash = self.selected_chain_store.read().get_by_index(curr_index).unwrap();

            let pchmr_proof_for_current = self.create_pchmr_witness(curr_hash, prev_hash)?;
            let prev_header: Arc<Header> = self.headers_store.get_header(prev_hash).unwrap();
            proof.push(PochmSegment { pchmr_witness: pchmr_proof_for_current, header: prev_header });
            (prev_hash, prev_index) = (curr_hash, curr_index);
            diff = prev_index - req_block_index;
        }
        Ok(proof)
    }
    pub fn verify_pochm_proof(&self, req_block_hash: Hash, witness: &Pochm) -> bool {
        //needs be relooked at
        let _post_posterity_header = &witness[0].header;
        let post_posterity_hash = _post_posterity_header.hash;
        if !self.verify_post_posterity_block(req_block_hash, post_posterity_hash) {
            return false;
        }

        let mut leaf_hashes = witness.iter().map(|pochm| pochm.header.hash).chain(std::iter::once(req_block_hash));
        leaf_hashes.next(); //remove first element to match accordingly to witnessess
        for (pochm, leaf_hash) in witness.iter().zip(leaf_hashes) {
            let pchmr = self.hash_to_pchmr_store.get(pochm.header.hash).unwrap();
            if !(self.verify_pchmr_witness(&pochm.pchmr_witness, leaf_hash, pchmr)) {
                return false;
            }
        }
        true
    }
    pub fn generate_tx_receipt(&self, req_block_hash: Hash, tracked_tx_id: Hash) -> Result<TxReceipt,ReceiptsError> {
        let pochm = self.create_pochm_proof(req_block_hash)?;
        let tx_acc_proof = self.create_merkle_witness_for_tx(tracked_tx_id, req_block_hash)?;
        Ok(TxReceipt { tracked_tx_id, accepting_block_hash: req_block_hash, pochm, tx_acc_proof })
    }
    pub fn generate_proof_of_pub(&self, req_block_hash: Hash, tracked_tx_id: Hash) -> Result<ProofOfPublication,ReceiptsError> {
        /*there is a certain degree of inefficiency here, as post_posterity is calculated again in create_pochm function
        however I expect this feature to rarely be called so optimizations seem not worth it */
        let post_posterity = self.get_post_posterity_block(req_block_hash)?;
        let tx_pub_proof = self.create_merkle_witness_for_tx(tracked_tx_id, req_block_hash)?;
        let mut headers_chain_to_selected = vec!();

        //find a chain block on the path to post_posterity
        for block in self.reachability_service.forward_chain_iterator(req_block_hash, post_posterity, true) {
            headers_chain_to_selected.push(self.headers_store.get_header(block).unwrap());
            if self.selected_chain_store.read().get_by_hash(block).is_ok() {
                break;
            }
        }
        let pochm = self.create_pochm_proof(headers_chain_to_selected.last().unwrap().hash)?;
        headers_chain_to_selected.remove(0); //remove the publishing block itself from the chain as it is redundant
        Ok(ProofOfPublication { tracked_tx_id, pub_block_hash: req_block_hash, pochm, tx_pub_proof, headers_chain_to_selected })
    }
    pub fn verify_tx_receipt(&self, tx_receipt: TxReceipt) -> bool {
        self.verify_merkle_witness_for_tx(&tx_receipt.tx_acc_proof, tx_receipt.tracked_tx_id, tx_receipt.accepting_block_hash)
            && self.verify_pochm_proof(tx_receipt.accepting_block_hash, &tx_receipt.pochm)
    }
    pub fn verify_proof_of_pub(&self, proof_of_pub: ProofOfPublication) -> bool {
        let pub_block_hash = proof_of_pub.pub_block_hash;
        let valid_path = proof_of_pub
            .headers_chain_to_selected
            .iter()
            .try_fold(pub_block_hash, |curr, next| if next.direct_parents().contains(&curr) { Some(next.hash) } else { None })
            .is_none();
        if !valid_path {
            return false;
        };

        let accepting_block_hash =
            proof_of_pub.headers_chain_to_selected.last().unwrap_or(&self.headers_store.get_header(pub_block_hash).unwrap()).hash;

        self.verify_merkle_witness_for_tx(&proof_of_pub.tx_pub_proof, proof_of_pub.tracked_tx_id, accepting_block_hash)
            && self.verify_pochm_proof(accepting_block_hash, &proof_of_pub.pochm)
    }

    pub fn get_post_posterity_block(&self, block_hash: Hash) -> Result<Hash,ReceiptsError> {
        //reach the first proceeding selected chain block
        let pruning_head = self.pruning_point_store.read().pruning_point().unwrap(); //may be inefficient
        let candidate_block = self
            .reachability_service
            .forward_chain_iterator(block_hash, pruning_head, true)
            .find(|&block| self.selected_chain_store.read().get_by_hash(block).is_ok()).ok_or(ReceiptsError::PostPosterityDoesNotExistYet(block_hash))?;

        let candidate_bscore: u64 = self.headers_store.get_blue_score(candidate_block).unwrap();
        let pruning_head_bscore = self.headers_store.get_blue_score(pruning_head).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth + self.posterity_depth;
        if cutoff_bscore > pruning_head_bscore {
            // Posterity block not yet available.
            Err(ReceiptsError::PostPosterityDoesNotExistYet(block_hash))
            } else 
            {
            self.get_posterity_by_bscore(candidate_block, cutoff_bscore).ok_or(ReceiptsError::PostPosterityDoesNotExistYet(block_hash))
        }
    }
    pub fn get_prev_posterity_block(&self, block_hash: Hash) -> Option<Hash> {
        //reach the first preceding selected chain block
        let candidate_block = self
            .reachability_service
            .default_backward_chain_iterator(block_hash)
            .find(|&block| self.selected_chain_store.read().get_by_hash(block).is_ok())?;

        let candidate_bscore: u64 = self.headers_store.get_blue_score(candidate_block).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth;

        self.get_posterity_by_bscore(candidate_block, cutoff_bscore)
    }

    fn get_posterity_by_bscore(&self, reference_block: Hash, cutoff_bscore: u64) -> Option<Hash> {
        //posterity_candidate is assumed to be a chain block
        //TODO:change to Error sometime probably
        /*returns  the first posterity block with bscore smaller or equal to the cutoff bscore
        assumes data is available*/

        if cutoff_bscore == 0
        // edge case
        {
            return Some(self.genesis.hash);
        }

        let mut low = 0;
        let mut high = self.selected_chain_store.read().get_tip().unwrap().0;
        let mut next_candidate = reference_block;
        let mut candidate_bscore = self.headers_store.get_blue_score(next_candidate).unwrap();

        // let mut next_index= self.selected_chain_store.read().get_by_hash(next_candidate).unwrap();
        let mut candidate_index = self.selected_chain_store.read().get_by_hash(next_candidate).unwrap();

        loop {
            /* a binary search 'style' loop
            with special checks to avoid getting stuck in a back and forth   */

            if candidate_bscore < cutoff_bscore {
                // in this case index will move forward
                if low < candidate_index
                //rescale bound and update
                {
                    low = candidate_index;
                    let index_diff = (cutoff_bscore - candidate_bscore) / self.average_width as u64;
                    candidate_index += index_diff;
                } else {
                    // if low bound was already known, we risk getting stuck in a loop, so just iterate forwards till posterity is found.
                    let high_block = self.selected_chain_store.read().get_by_index(high).unwrap();
                    return self
                        .reachability_service
                        .forward_chain_iterator(next_candidate, high_block, true)
                        .find(|&block| self.headers_store.get_blue_score(block).unwrap() >= cutoff_bscore);
                }
            } else {
                // in this case index will move backward
                if high > candidate_index {
                    let candidate_parent = self.reachability_service.get_chain_parent(next_candidate);
                    let candidate_parent_bscore = self.headers_store.get_blue_score(candidate_parent).unwrap();
                    if candidate_parent_bscore < cutoff_bscore
                    // first check if next_candidate actually is the posterity
                    {
                        return Some(next_candidate);
                    } else {
                        // if not, update candidate indices and bounds
                        let index_diff = (candidate_bscore - cutoff_bscore) / self.average_width as u64;
                        candidate_index -= index_diff; //shouldn't overflow
                        high = candidate_index; // if not, rescale bound
                    }
                } else {
                    //again avoid getting stuck in a loop
                    //iterate back until a parent is found with blue score lower than the cutoff
                    let low_block = self.selected_chain_store.read().get_by_index(low).unwrap();
                    let posterity_parent = self
                        .reachability_service
                        .backward_chain_iterator(next_candidate, low_block, true)
                        .find(|&block| self.headers_store.get_blue_score(block).unwrap() < cutoff_bscore)?;
                    // and then return its 'selected' son
                    return self.reachability_service.forward_chain_iterator(posterity_parent, next_candidate, true).next();
                }
            }
            next_candidate = self.selected_chain_store.read().get_by_index(candidate_index).unwrap();
            candidate_bscore = self.headers_store.get_blue_score(next_candidate).unwrap();
        }
    }
    pub fn verify_post_posterity_block(&self, block_hash: Hash, post_posterity_candidate_hash: Hash) -> bool {
        /*the verification consists of 3 parts:
        1) verify the block queried is an ancesstor of the candidate
        2)verify the candidate is on the selected chain
        3) verify the selected parent of the candidate has blue score score smaller than the posterity designated blue score*/
        if !self.reachability_service.is_dag_ancestor_of(block_hash, post_posterity_candidate_hash) {
            return false;
        }
        if self.selected_chain_store.read().get_by_hash(post_posterity_candidate_hash).is_ok() {
            return false;
        }
        let candidate_bscore = self.headers_store.get_blue_score(post_posterity_candidate_hash).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth;
        let candidate_sel_parent_hash =
            self.reachability_service.default_backward_chain_iterator(post_posterity_candidate_hash).next().unwrap();
        let candidate_sel_parent_bscore = self.headers_store.get_blue_score(candidate_sel_parent_hash).unwrap();
        candidate_sel_parent_bscore < cutoff_bscore
    }
}
#[derive(Clone)]
pub struct PochmSegment {
    pchmr_witness: MerkleWitness,
    header: Arc<Header>,
}
pub type Pochm = Vec<PochmSegment>;
pub struct TxReceipt {
    tracked_tx_id: Hash,
    accepting_block_hash: Hash,
    pochm: Pochm,
    tx_acc_proof: MerkleWitness,
}
pub struct ProofOfPublication {
    tracked_tx_id: Hash,
    pub_block_hash: Hash,
    pochm: Pochm,
    tx_pub_proof: MerkleWitness,
    headers_chain_to_selected: Vec<Arc<Header>>,
}
