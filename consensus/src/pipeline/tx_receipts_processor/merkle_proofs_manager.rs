use super::{
    error::ReceiptsErrors,
    pchmr_store::{DbPchmrStore, PchmrStoreReader},
    rep_parents_store::DbRepParentsStore,
};
use crate::model::stores::selected_chain::SelectedChainStoreReader;
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
use kaspa_hashes::ZERO_HASH;
use kaspa_merkle::{
    calc_merkle_root, create_merkle_witness_from_sorted, create_merkle_witness_from_unsorted, verify_merkle_witness, MerkleWitness,
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
pub struct MerkleProofsManager<T: SelectedChainStoreReader, U: ReachabilityStoreReader, V: HeaderStoreReader> {
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

    // Stores
    // pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    // pub(super) ghostdag_primary_store: Arc<DbGhostdagStore>,
    headers_store: Arc<V>,
    selected_chain_store: Arc<RwLock<T>>, // pub(super) daa_excluded_store: Arc<DbDaaStore>,
    // pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    // pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,
    // pub(super) depth_store: Arc<DbDepthStore>,
    pub(super) hash_to_pchmr_store: Arc<DbPchmrStore>,
    pub(super) rep_parents_store: Arc<DbRepParentsStore>, //will probably remove field

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
    pub(super) reachability_service: MTReachabilityService<U>, // pub(super) relations_service: MTRelationsService<DbRelationsStore>,
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

impl<T: SelectedChainStoreReader, U: ReachabilityStoreReader, V: HeaderStoreReader> MerkleProofsManager<T, U, V> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        // receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
        // pruning_sender: CrossbeamSender<PruningProcessingMessage>,
        // pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,
        // thread_pool: Arc<ThreadPool>,
        params: &Params,
        // db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        dag_traversal_manager: DbDagTraversalManager,
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

            // db,
            // statuses_store: storage.statuses_store.clone(),
            headers_store,
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
    pub fn generate_tx_receipt(&self, req_block_hash: Hash, tracked_tx_id: Hash) -> Result<TxReceipt, ReceiptsErrors> {
        let pochm = self.create_pochm_proof(req_block_hash)?;
        let tx_acc_proof = self.create_merkle_witness_for_tx(tracked_tx_id, req_block_hash)?;
        Ok(TxReceipt { tracked_tx_id, accepting_block_hash: req_block_hash, pochm, tx_acc_proof })
    }
    pub fn generate_proof_of_pub(&self, req_block_hash: Hash, tracked_tx_id: Hash) -> Result<ProofOfPublication, ReceiptsErrors> {
        /*there is a certain degree of inefficiency here, as post_posterity is calculated again in create_pochm function
        however I expect this feature to rarely be called so optimizations seem not worth it */
        let post_posterity = self.get_post_posterity_block(req_block_hash)?;
        let tx_pub_proof = self.create_merkle_witness_for_tx(tracked_tx_id, req_block_hash)?;
        let mut headers_chain_to_selected = vec![];

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
    pub fn create_pochm_proof(&self, req_block_hash: Hash) -> Result<Pochm, ReceiptsErrors> {
        /*Assumes: requested block hash is on the selected chain,
        if not returns error   */
        let mut pochm_proof = Pochm::new(self.hash_to_pchmr_store.clone());
        let post_posterity_hash = self.get_post_posterity_block(req_block_hash)?;
        let req_block_index = self
            .selected_chain_store
            .read()
            .get_by_hash(req_block_hash)
            .map_err(|_| ReceiptsErrors::RequestedBlockNotOnSelectedChain(req_block_hash))?;
        let (mut root_block_hash, mut root_block_index) =
            (post_posterity_hash, self.selected_chain_store.read().get_by_hash(post_posterity_hash).unwrap()); //if posterity block is not on selected chain, panic.
        let mut leaf_block_index;
        let mut remaining_index_diff = root_block_index - req_block_index;
        let mut leaf_block_hash;
        while remaining_index_diff > 0 {
            leaf_block_index = root_block_index - (remaining_index_diff + 1).next_power_of_two() / 2; //subtract highest possible power of two such as to not cross 0
            leaf_block_hash = self.selected_chain_store.read().get_by_index(leaf_block_index)?;

            let leaf_is_in_pchmr_of_root_proof = self.create_pchmr_witness(leaf_block_hash, root_block_hash)?;
            let root_block_header: Arc<Header> = self.headers_store.get_header(root_block_hash).unwrap();
            pochm_proof.insert(root_block_header, leaf_is_in_pchmr_of_root_proof);

            (root_block_hash, root_block_index) = (leaf_block_hash, leaf_block_index);
            remaining_index_diff = root_block_index - req_block_index;
        }
        Ok(pochm_proof)
    }
    pub fn verify_pochm_proof(&self, req_block_hash: Hash, witness: &Pochm) -> bool {
        /*this function will return true for any witness premiering with a currently non pruned block and
        recursively pointing down to req_block_hash, it is the responsibility of the
        creator of the witness to make sure the witness premiers with a posterity block
        and not just any block that may be pruned in the future, as this property is not verified in this function,
        and the function should not be relied upon to confirm the witness is everlasting*/

        if let Some(post_posterity_hash) = witness.get_path_origin() {
            if self.headers_store.get_header(post_posterity_hash).is_ok()
            // verify the corresponding header is available
            {
                //verification of path itself is delegated to the pochm struct
                return witness.verify_pchmrs_path(req_block_hash);
            }
        }
        false
    }
    pub fn calc_pchmr_root(&self, req_selected_parent: Hash) -> Hash {
        /*  function receives the selected parent of the relevant block,
        as the block itself at this point is not assumed to exist*/
        let representative_parents_list = self.representative_log_parents(req_selected_parent);
        calc_merkle_root(representative_parents_list.into_iter())
        // in block template: should update rep_parents_store
    }
    pub fn create_pchmr_witness(&self, leaf_block_hash: Hash, root_block_hash: Hash) -> Result<MerkleWitness, ReceiptsErrors> {
        // proof that a block belongs to the pchmr tree of another block
        /* the function assumes that the path from block_hash down to its posterity is intact and has not been pruned
        (which should be the same as assuming block_hash has not been pruned)
        it will panic if not.*/
        let parent_of_root = self.reachability_service.get_chain_parent(root_block_hash);
        let log_sized_parents_list = self.representative_log_parents(parent_of_root);

        create_merkle_witness_from_unsorted(log_sized_parents_list.into_iter(), leaf_block_hash).map_err(|e| e.into())
    }
    pub fn verify_pchmr_witness(&self, witness: &MerkleWitness, leaf_block_hash: Hash, root_block_hash: Hash) -> bool {
        verify_merkle_witness(witness, leaf_block_hash, self.hash_to_pchmr_store.get(root_block_hash).unwrap())
    }

    pub fn create_merkle_witness_for_tx(&self, tracked_tx_id: Hash, req_block_hash: Hash) -> Result<MerkleWitness, ReceiptsErrors> {
        let mergeset_txs_manager = self.acceptance_data_store.get(req_block_hash)?;
        let mut accepted_txs = vec![];

        for parent_acc_data in mergeset_txs_manager.iter() {
            let parent_acc_txs = parent_acc_data.accepted_transactions.iter().map(|tx| tx.transaction_id);

            for tx in parent_acc_txs {
                accepted_txs.push(tx);
            }
        }
        accepted_txs.sort();

        create_merkle_witness_from_sorted(accepted_txs.into_iter(), tracked_tx_id).map_err(|e| e.into())
    }
    pub fn verify_merkle_witness_for_tx(&self, witness: &MerkleWitness, tracked_tx_id: Hash, req_block_hash: Hash) -> bool {
        // maybe make it return result? rethink
        let mergeset_txs_manager = self.acceptance_data_store.get(req_block_hash); // I think this is incorrect
        if mergeset_txs_manager.is_err() {
            return false;
        }
        let req_block_header = self.headers_store.get_header(req_block_hash).unwrap();
        let req_atmr = req_block_header.accepted_id_merkle_root;
        verify_merkle_witness(witness, tracked_tx_id, req_atmr)
    }
    fn representative_log_parents(&self, req_block_parent: Hash) -> Vec<Hash> {
        /* the function assumes that the path from block_hash down to its posterity is intact and has not been pruned
        (which should be the same as assuming block_hash has not been pruned)
        it will panic if not.
        Function receives the selected parent of the relevant block ,as the block itself is not assumed to necessarily exist
        Returns all 2^i deep 'selected' parents up to the posterity block not included */
        let pre_posterity_hash = self.get_pre_posterity_block(req_block_parent);
        let pre_posterity_bscore = self.headers_store.get_blue_score(pre_posterity_hash).unwrap();
        let mut representative_parents_list = vec![];
        /*The following logic will not be efficient for blocks which are a long distance away from the selected chain,
        Hence, the corresponding field for which this calculation should only be verified for selected chain candidates
        This function is also called when creating said field - in this case however any honest node should only call for it on a block which
        would be on the selected chain from its point of view
        nethertheless, the logic will return a correct answer if called.
         */
        let mut distance_covered_before_chain = 0; //compulsory initialization for compiler only , a chain block will have to be reached eventually
        let mut first_chain_block = ZERO_HASH; //compulsory initialization for compiler only, a chain block will have to be reached eventually
        for (i, current) in self.reachability_service.default_backward_chain_iterator(req_block_parent).enumerate() {
            let index = i + 1; //enumeration should start from 1
            if current == pre_posterity_hash {
                // pre posterity not included in list
                return representative_parents_list;
            } else if self.selected_chain_store.read().get_by_hash(current).is_ok() {
                // get out of loop and apply selected chain logic instead
                first_chain_block = current;
                distance_covered_before_chain = i as u64;
                break;
            } else if (index & (index - 1)) == 0 {
                //trickery to check if index is a power of two
                representative_parents_list.push(current);
            }
        }
        let first_chain_block_index = self.selected_chain_store.read().get_by_hash(first_chain_block).unwrap();
        let req_block_imaginary_index = first_chain_block_index + distance_covered_before_chain;
        let mut next_power = distance_covered_before_chain.next_power_of_two();
        let mut next_chain_block_rep_parent =
            self.selected_chain_store.read().get_by_index(req_block_imaginary_index - next_power).unwrap();
        let mut next_bscore = self.headers_store.get_blue_score(next_chain_block_rep_parent).unwrap();
        while next_bscore > pre_posterity_bscore {
            representative_parents_list.push(next_chain_block_rep_parent);
            next_power *= 2;
            next_chain_block_rep_parent = self
                .selected_chain_store
                .read()
                .get_by_index(first_chain_block_index.saturating_sub(next_power - distance_covered_before_chain))
                .unwrap();
            next_bscore = self.headers_store.get_blue_score(next_chain_block_rep_parent).unwrap();
        }
        representative_parents_list
    }

    pub fn get_post_posterity_block(&self, block_hash: Hash) -> Result<Hash, ReceiptsErrors> {
        /* the function assumes that the path from block_hash up to its post posterity if it exits is intact and has not been pruned
        (which should be the same as assuming block_hash has not been pruned)
        it will panic if not.
        An error is returned if post_posterity does not yet exist.
        The function does not assume block_hash is a chain block, however
        the known aplications of posterity blocks appear nonsensical when it is not.
         */
        let block_bscore: u64 = self.headers_store.get_blue_score(block_hash).unwrap();
        let tentative_cutoff_bscore = block_bscore - block_bscore % self.posterity_depth + self.posterity_depth;
        let pruning_head = self.pruning_point_store.read().pruning_point()?; //possibly inefficient
                                                                             /*try and reach the first proceeding selected chain block,
                                                                             while checking if pre_posterity of queried block is of the rare case where it is encountered before arriving at a chain block
                                                                             in the majority of cases, a very short distance is covered before reaching a chain block.
                                                                             */
        let candidate_block = self
            .reachability_service
            .forward_chain_iterator(block_hash, pruning_head, true)
            .find(|&block| {
                let block_bscore = self.headers_store.get_blue_score(block).unwrap();
                block_bscore > tentative_cutoff_bscore || self.selected_chain_store.read().get_by_hash(block).is_ok()
            })
            .ok_or(ReceiptsErrors::PosterityDoesNotExistYet(tentative_cutoff_bscore))?;

        let candidate_bscore: u64 = self.headers_store.get_blue_score(candidate_block).unwrap();
        if candidate_bscore > tentative_cutoff_bscore {
            // in case cutoff_bscore was crossed prior to reaching a chain block
            return Ok(candidate_block);
        }
        let pruning_head_bscore = self.headers_store.get_blue_score(pruning_head).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth + self.posterity_depth;
        if cutoff_bscore > pruning_head_bscore {
            // Posterity block not yet available.
            Err(ReceiptsErrors::PosterityDoesNotExistYet(cutoff_bscore))
        } else {
            let witdth_guess = 2; //hardcoded guess
            self.get_chain_block_posterity_by_bscore(candidate_block, cutoff_bscore, Some(witdth_guess))
        }
    }
    pub fn get_pre_posterity_block(&self, block_hash: Hash) -> Hash {
        /* the function assumes that the path from block_hash down to its posterity is intact and has not been pruned
        (which should be the same as assuming block_hash has not been pruned)
        it will panic if not.
        The function does not assume block_hash is a chain block, however
        the known aplications of posterity blocks appear nonsensical when it is not
        */
        let block_bscore: u64 = self.headers_store.get_blue_score(block_hash).unwrap();
        let tentative_cutoff_bscore = block_bscore - block_bscore % self.posterity_depth;
        if tentative_cutoff_bscore == 0
        //genesis block edge case
        {
            return self.genesis.hash;
        }
        /*try and reach the first preceding selected chain block,
        while checking if pre_posterity of queried block is of the rare case
        where it is encountered before arriving at a chain block
        in the majority of cases, a very short distance is covered before reaching a chain block
        */
        let candidate_block = self
            .reachability_service
            .default_backward_chain_iterator(block_hash)
            .find(|&block| {
                self.headers_store.get_blue_score(block).unwrap() < tentative_cutoff_bscore
                    || self.selected_chain_store.read().get_by_hash(block).is_ok()
            })
            .unwrap();
        // in case cutoff_bscore was crossed prior to reaching a chain block
        if self.headers_store.get_blue_score(candidate_block).unwrap() < tentative_cutoff_bscore {
            let posterity_parent = candidate_block;
            return self.reachability_service.forward_chain_iterator(posterity_parent, block_hash, true)
            .nth(1)             //skip posterity parent
            .unwrap();
        }
        //othrewise, recalculate the cutoff score in accordance to the candiate
        let candidate_bscore: u64 = self.headers_store.get_blue_score(candidate_block).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth;
        let witdth_guess = 2; //hardcoded guess

        self.get_chain_block_posterity_by_bscore(candidate_block, cutoff_bscore, Some(witdth_guess)).unwrap()
    }

    fn get_chain_block_posterity_by_bscore(
        &self,
        reference_block: Hash,
        cutoff_bscore: u64,
        width_guess: Option<u64>,
    ) -> Result<Hash, ReceiptsErrors> {
        //reference_block is assumed to be a chain block
        /*returns  the first posterity block with bscore smaller or equal to the cutoff bscore
        assumes data is available, will panic if not*/

        if cutoff_bscore == 0
        // edge case
        {
            return Ok(self.genesis.hash);
        }
        let mut low = self.selected_chain_store.read()
        .get_by_hash(self.pruning_point_store.read().history_root().unwrap()).unwrap();
        let mut high = self.selected_chain_store.read().get_tip().unwrap().0;
        let mut next_candidate = reference_block;
        let mut index_step;
        let mut candidate_bscore = self.headers_store.get_blue_score(next_candidate).unwrap();
        // let mut next_index= self.selected_chain_store.read().get_by_hash(next_candidate).unwrap();
        let mut candidate_index = self.selected_chain_store.read().get_by_hash(next_candidate).unwrap();
        let mut estimated_width = width_guess.unwrap_or(candidate_bscore / candidate_index); // a very rough estimation in case None was given

        if high < candidate_index {
            return Err(ReceiptsErrors::PosterityDoesNotExistYet(cutoff_bscore));
        };
        loop {
            /* a binary search 'style' loop
            with special checks to avoid getting stuck in a back and forth
            Suggestion: make recursive   */

            /*Special attention is taken to prevent a 0 value from occuring
            and causing divide by 0 or lack of progress
            estimated width is guaranteed a non zero value*/
            index_step = (candidate_bscore.abs_diff(cutoff_bscore)) / estimated_width;
            index_step = if index_step == 0 { 1 } else { index_step };
            if candidate_bscore < cutoff_bscore {
                // in this case index will move forward
                if low < candidate_index
                //rescale bound and update
                {
                    low = candidate_index; //rescale bound
                    candidate_index += index_step;
                } else {
                    // if low bound was already known, we risk getting stuck in a loop, so just iterate forwards till posterity is found.
                    let high_block = self.selected_chain_store.read().get_by_index(high).unwrap();
                    return Ok(self
                        .reachability_service
                        .forward_chain_iterator(next_candidate, high_block, true)
                        .find(|&block| self.headers_store.get_blue_score(block).unwrap() >= cutoff_bscore)
                        .unwrap());
                }
            } else {
                // in this case index will move backward
                if high > candidate_index {
                    let candidate_parent = self.reachability_service.get_chain_parent(next_candidate);
                    let candidate_parent_bscore = self.headers_store.get_blue_score(candidate_parent).unwrap();
                    if candidate_parent_bscore < cutoff_bscore
                    // first check if next_candidate actually is the posterity
                    {
                        return Ok(next_candidate);
                    } else {
                        // if not, update candidate indices and bounds
                        high = candidate_index; //  rescale bound
                        candidate_index -= index_step; //shouldn't overflow
                    }
                } else {
                    //again avoid getting stuck in a loop
                    //iterate back until a parent is found with blue score lower than the cutoff
                    let low_block = self.selected_chain_store.read().get_by_index(low).unwrap();
                    let posterity_parent = self
                        .reachability_service
                        .backward_chain_iterator(next_candidate, low_block, true)
                        .find(|&block| self.headers_store.get_blue_score(block).unwrap() < cutoff_bscore)
                        .unwrap();
                    // and then return its 'selected' son
                    return Ok(self
                        .reachability_service
                        .forward_chain_iterator(posterity_parent, next_candidate, true)
                        .nth(1)//skip posterity parent
                        .unwrap());
                }
            }
            next_candidate = self.selected_chain_store.read().get_by_index(candidate_index).unwrap();
            let candidate_bscore_next = self.headers_store.get_blue_score(next_candidate).unwrap();
            /*update the estimated width based on the latest result
            Notice a 0 value can never occur:
            A) because index_step!=0, meaning candidate_bscore_next and candidate_bscore are strictly different
            B) because |candidate_bscore_next-candidate_bscore| is by definition the minimal possible value index_step can get
            divide by 0 doesn't occur since index_step!=0*/
            estimated_width = (candidate_bscore.abs_diff(candidate_bscore_next)) / index_step;
            assert_ne!(estimated_width, 0);

            candidate_bscore = candidate_bscore_next;
        }
    }
    pub fn verify_post_posterity_block(&self, block_hash: Hash, post_posterity_candidate_hash: Hash) -> bool {
        /*the verification consists of 3 parts:
        1) verify the block queried is an ancesstor of the candidate
        2)verify the candidate is on the selected chain
        3) verify the selected parent of the candidate has blue score score smaller than the posterity designated blue score
        function hence assumes the selected parent has not been pruned
        */
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
    header: Arc<Header>,
    leaf_in_pchmr_witness: MerkleWitness,
}
#[derive(Clone)]
pub struct Pochm {
    vec: Vec<PochmSegment>,
    hash_to_pchmr_store: Arc<DbPchmrStore>, //temporary field
}
impl Pochm {
    pub fn new(hash_to_pchmr_store: Arc<DbPchmrStore>) -> Self {
        let vec = vec![];
        Self { vec, hash_to_pchmr_store }
    }

    pub fn insert(&mut self, header: Arc<Header>, witness: MerkleWitness) {
        self.vec.push(PochmSegment { header, leaf_in_pchmr_witness: witness })
    }
    pub fn get_path_origin(&self) -> Option<Hash> {
        self.vec.first().map(|seg| seg.header.hash)
    }

    pub fn verify_pchmrs_path(&self, destination_block_hash: Hash) -> bool {
        let leaf_hashes = self.vec.iter()
        .skip(1)//remove first element to match accordingly to witnesses 
        .map(|pochm| pochm.header.hash)//map to hashes
        .chain(std::iter::once(destination_block_hash)); // add final block

        /*verify the path from posterity down to req_block_hash:
        iterate downward from posterity block header: for each, check that leaf hash is  */
        for (pochm_seg, leaf_hash) in self.vec.iter().zip(leaf_hashes) {
            let pchmr_root_hash = self.hash_to_pchmr_store.get(pochm_seg.header.hash).unwrap();
            let witness = &pochm_seg.leaf_in_pchmr_witness;
            if !(verify_merkle_witness(witness, leaf_hash, pchmr_root_hash)) {
                return false;
            }
        }
        true
    }
}
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
