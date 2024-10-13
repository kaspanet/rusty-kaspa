use super::{
    pchmr_store::{DbPchmrStore, PchmrStore, PchmrStoreReader},
    rep_parents_store::{DbRepParentsStore, RepParentsStoreReader},
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
            reachability::DbReachabilityStore,
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
use kaspa_consensus_core::blockhash::ORIGIN;
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

pub struct Kip6Processor {
    // Channels
    receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
    pruning_sender: CrossbeamSender<PruningProcessingMessage>,
    pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) max_block_parents: u8,
    pub(super) mergeset_size_limit: u64,
    pub(super) pruning_depth: u64,
    pub(super) posterity_depth: u64,
    pub(super) average_width: u64,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_primary_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) daa_excluded_store: Arc<DbDaaStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,
    pub(super) depth_store: Arc<DbDepthStore>,
    pub(super) selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
    pub(super) hash_to_pchmr_store: Arc<DbPchmrStore>,
    pub(super) rep_parents_store: Arc<DbRepParentsStore>,

    // Utxo-related stores
    pub(super) utxo_diffs_store: Arc<DbUtxoDiffsStore>,
    pub(super) utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    pub(super) virtual_stores: Arc<RwLock<VirtualStores>>,
    pub(super) pruning_utxoset_stores: Arc<RwLock<PruningUtxosetStores>>,

    /// The "last known good" virtual state. To be used by any logic which does not want to wait
    /// for a possible virtual state write to complete but can rather settle with the last known state
    pub lkg_virtual_state: LkgVirtualState,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) relations_service: MTRelationsService<DbRelationsStore>,
    pub(super) dag_traversal_manager: DbDagTraversalManager,
    pub(super) window_manager: DbWindowManager,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) pruning_point_manager: DbPruningPointManager,
    pub(super) parents_manager: DbParentsManager,
    pub(super) depth_manager: DbBlockDepthManager,

    // Pruning lock
    pruning_lock: SessionLock,

    // Notifier
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,

    // Storage mass hardfork DAA score
    pub(crate) storage_mass_activation_daa_score: u64,
}

impl Kip6Processor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
        pruning_sender: CrossbeamSender<PruningProcessingMessage>,
        pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        params: &Params,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,
        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        hash_to_pchmr_store: Arc<DbPchmrStore>,
        rep_parents_store: Arc<DbRepParentsStore>,
    ) -> Self {
        Self {
            receiver,
            pruning_sender,
            pruning_receiver,
            thread_pool,

            genesis: params.genesis.clone(),
            max_block_parents: params.max_block_parents,
            mergeset_size_limit: params.mergeset_size_limit,
            pruning_depth: params.pruning_depth,
            posterity_depth: params.pruning_depth,
            average_width: 2, //hardcoded, should advise with others if this is the correct solution

            db,
            statuses_store: storage.statuses_store.clone(),
            headers_store: storage.headers_store.clone(),
            ghostdag_primary_store: storage.ghostdag_primary_store.clone(),
            daa_excluded_store: storage.daa_excluded_store.clone(),
            block_transactions_store: storage.block_transactions_store.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            depth_store: storage.depth_store.clone(),
            selected_chain_store: storage.selected_chain_store.clone(),
            utxo_diffs_store: storage.utxo_diffs_store.clone(),
            utxo_multisets_store: storage.utxo_multisets_store.clone(),
            acceptance_data_store: storage.acceptance_data_store.clone(),
            virtual_stores: storage.virtual_stores.clone(),
            pruning_utxoset_stores: storage.pruning_utxoset_stores.clone(),
            lkg_virtual_state: storage.lkg_virtual_state.clone(),

            ghostdag_manager: services.ghostdag_primary_manager.clone(),
            reachability_service: services.reachability_service.clone(),
            relations_service: services.relations_service.clone(),
            dag_traversal_manager: services.dag_traversal_manager.clone(),
            window_manager: services.window_manager.clone(),
            coinbase_manager: services.coinbase_manager.clone(),
            transaction_validator: services.transaction_validator.clone(),
            pruning_point_manager: services.pruning_point_manager.clone(),
            parents_manager: services.parents_manager.clone(),
            depth_manager: services.depth_manager.clone(),

            pruning_lock,
            notification_root,
            counters,
            storage_mass_activation_daa_score: params.storage_mass_activation_daa_score,
            hash_to_pchmr_store,
            rep_parents_store,
        }
    }

    pub fn create_merkle_witness_for_tx(&self, tracked_tx_id: Hash, req_block_hash: Hash) -> Option<MerkleWitness> {
        // maybe better here to make it return result? rethink
        let mergeset_txs_manager = self.acceptance_data_store.get(req_block_hash); // I think this is incorrect
        if mergeset_txs_manager.is_err() {
            return None;
        };
        let mergeset_txs_manager = mergeset_txs_manager.unwrap();
        let mut accepted_txs = vec![];

        for parent_acc_data in mergeset_txs_manager.iter() {
            let parent_acc_txs = parent_acc_data.accepted_transactions.iter().map(|tx| tx.transaction_id);

            for tx in parent_acc_txs {
                accepted_txs.push(tx);
            }
        }
        accepted_txs.sort();

        create_merkle_witness_from_sorted(accepted_txs.into_iter(), tracked_tx_id)
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
    pub fn representative_log_parents(&self, req_block_hash: Hash) -> Vec<Hash> {
        //returns all 2^i deep parents up to the posterity block
        let pre_posterity_hash = self.get_prev_posterity_block(req_block_hash);
        let pre_posterity_daa = self.headers_store.get_daa_score(pre_posterity_hash).unwrap();
        let mut representative_parents_list = vec![];
        /*  currently I store the representative parents of each block in memory to implement this efficiently,
        there likely are better ways, but we might change to a completely different method later on
        so currently not worth to much thought*/
        let mut i = 0;
        let mut curr_block = self.reachability_service.default_backward_chain_iterator(req_block_hash).next();
        while curr_block.is_some() && self.headers_store.get_daa_score(pre_posterity_hash).unwrap() > pre_posterity_daa {
            representative_parents_list.push(curr_block.unwrap());
            curr_block = self.rep_parents_store.get_ith_rep_parent(curr_block.unwrap(), i);
            i += 1;
        }
        // for (i, current) in self.reachability_service.default_backward_chain_iterator(req_block_hash).enumerate() {
        //     let index = i + 1; //enumeration should start from 1
        //     if current == pre_posterity_hash {
        //         break;
        //     }
        //     if (index & (index - 1)) == 0 {
        //         //trickery to check if index is a power of two
        //         representative_parents_list.push(current);
        //     }
        //     continue;
        // }
        representative_parents_list
    }
    pub fn calc_pchmr_root(&self, req_block_hash: Hash) -> Hash {
        let log_sized_parents_list = self.representative_log_parents(req_block_hash);
        let ret = calc_merkle_root(log_sized_parents_list.into_iter());
        //temporary non hard fork solution
        self.hash_to_pchmr_store.insert(req_block_hash, ret).unwrap();
        ret
    }
    pub fn create_pchmr_witness(&self, leaf_block_hash: Hash, root_block_hash: Hash) -> Option<MerkleWitness> {
        // proof that a block belongs to the prhmr tree of another block
        let log_sized_parents_list = self.representative_log_parents(root_block_hash);

        create_merkle_witness_from_unsorted(log_sized_parents_list.into_iter(), leaf_block_hash)
    }
    pub fn verify_pchmr_witness(&self, witness: &MerkleWitness, leaf_block_hash: Hash, root_block_hash: Hash) -> bool {
        verify_merkle_witness(witness, leaf_block_hash, self.hash_to_pchmr_store.get(root_block_hash).unwrap())
    }
    pub fn create_pochm_proof(&self, req_block_hash: Hash) -> Option<Pochm> {
        //needs be relooked at
        let mut proof = vec![];
        let post_posterity_hash = self.get_post_posterity_block(req_block_hash).unwrap();
        let mut prev_hash = post_posterity_hash;
        for (i, current_hash) in self.reachability_service.default_backward_chain_iterator(post_posterity_hash).enumerate() {
            let index = i + 1; //enumeration should start from 1
            if current_hash == req_block_hash || (index & (index - 1)) == 0 {
                //trickery to check if it is a power of two
                let pmr_proof_for_current = self.create_pchmr_witness(current_hash, prev_hash)?;
                let prev_header: Arc<Header> = self.headers_store.get_header(prev_hash).unwrap();
                proof.push(PochmSegment { pmr_witness: pmr_proof_for_current, header: prev_header });
                prev_hash = current_hash;
            }
            if current_hash == req_block_hash {
                break;
            }
        }
        Some(proof)
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
            if !(self.verify_pchmr_witness(&pochm.pmr_witness, leaf_hash, pchmr)) {
                return false;
            }
        }
        true
    }
    pub fn generate_tx_receipt(&self, req_block_hash: Hash, tracked_tx_id: Hash) -> Option<TxReceipt> {
        let pochm = self.create_pochm_proof(req_block_hash)?;
        let tx_acc_proof = self.create_merkle_witness_for_tx(tracked_tx_id, req_block_hash)?;
        Some(TxReceipt { tracked_tx_id, accepting_block_hash: req_block_hash, pochm, tx_acc_proof })
    }
    pub fn generate_proof_of_pub(&self, req_block_hash: Hash, tracked_tx_id: Hash) -> Option<ProofOfPublication> {
        let pruning_head = self.pruning_point_store.read().pruning_point().unwrap(); //this appears incorrect, and requires further thought

        let tx_pub_proof = self.create_merkle_witness_for_tx(tracked_tx_id, req_block_hash)?;
        let mut headers_chain_to_selected = vec![];
        for block in self.reachability_service.forward_chain_iterator(req_block_hash, pruning_head, true) {
            headers_chain_to_selected.push(self.headers_store.get_header(block).unwrap());
            if self.selected_chain_store.read().get_by_hash(block).is_ok() {
                break;
            }
        }
        let pochm = self.create_pochm_proof(headers_chain_to_selected.last().unwrap().hash)?;
        headers_chain_to_selected.remove(0); //remove the publishing block itself from the chain as it is redundant
        Some(ProofOfPublication { tracked_tx_id, pub_block_hash: req_block_hash, pochm, tx_pub_proof, headers_chain_to_selected })
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

    pub fn get_post_posterity_block(&self, block_hash: Hash) -> Option<Hash> {
        let pruning_head = self.pruning_point_store.read().pruning_point().unwrap();
        let candidate_block = self
            .reachability_service
            .forward_chain_iterator(block_hash, pruning_head, true)
            .find(|&block| self.selected_chain_store.read().get_by_hash(block).is_ok())
            .unwrap();

        let candidate_daa: u64 = self.headers_store.get_daa_score(candidate_block).unwrap();
        let pruning_head_daa = self.headers_store.get_daa_score(pruning_head).unwrap();
        let cutoff_daa = candidate_daa - candidate_daa % self.posterity_depth + self.posterity_depth;
        if cutoff_daa > pruning_head_daa {
            None
        } else {
            Some(self.get_posterity_by_daa(candidate_block, cutoff_daa))
        }
    }
    pub fn get_prev_posterity_block(&self, block_hash: Hash) -> Hash {
        //possibly pruning point points too far back.
        let candidate_block = self
            .reachability_service
            .backward_chain_iterator(block_hash, ORIGIN, true)
            .find(|&block| self.selected_chain_store.read().get_by_hash(block).is_ok())
            .unwrap();

        let candidate_daa: u64 = self.headers_store.get_daa_score(candidate_block).unwrap();
        let cutoff_daa = candidate_daa - candidate_daa % self.posterity_depth;

        self.get_posterity_by_daa(candidate_block, cutoff_daa)
    }

    pub fn get_posterity_by_daa(&self, posterity_candidate: Hash, cutoff_daa: u64) -> Hash {
        //TODO:change to Error sometime probably
        /*returns  the first posterity block with daa smaller or equal to the approximate daa
        assumes data is available*/
        let candidate_parent = self.reachability_service.get_chain_parent(posterity_candidate);

        let candidate_daa = self.headers_store.get_daa_score(posterity_candidate).unwrap();
        let candidate_parent_daa = self.headers_store.get_daa_score(candidate_parent).unwrap();
        if candidate_daa >= cutoff_daa || candidate_parent_daa < cutoff_daa {
            return posterity_candidate;
        }
        //dangerous conversion magic
        let index_diff: i128 = (candidate_daa as i128 - cutoff_daa as i128) / (self.average_width as i128);
        let reference_index: i128 = self.selected_chain_store.read().get_by_hash(posterity_candidate).unwrap() as i128;
        let mut next_index = 0;
        if reference_index > index_diff {
            next_index = (reference_index - index_diff) as u64
        }
        let next_candidate: Hash = self.selected_chain_store.read().get_by_index(next_index).unwrap();
        self.get_posterity_by_daa(next_candidate, cutoff_daa)
    }
    pub fn verify_post_posterity_block(&self, block_hash: Hash, post_posterity_candidate_hash: Hash) -> bool {
        /*the verification consists of 3 parts:
        1) verify the block queried is an ancesstor of the candidate
        2)verify the candidate is on the selected chain
        3) verify the selected parent of the candidate has daa score smaller than the posterity designated daa*/
        if !self.reachability_service.is_dag_ancestor_of(block_hash, post_posterity_candidate_hash) {
            return false;
        }
        if self.selected_chain_store.read().get_by_hash(post_posterity_candidate_hash).is_ok() {
            return false;
        }
        let candidate_daa = self.headers_store.get_daa_score(post_posterity_candidate_hash).unwrap();
        let cutoff_daa = candidate_daa - candidate_daa % self.posterity_depth;
        let candidate_sel_parent_hash =
            self.reachability_service.default_backward_chain_iterator(post_posterity_candidate_hash).next().unwrap();
        let candidate_sel_parent_daa = self.headers_store.get_daa_score(candidate_sel_parent_hash).unwrap();
        candidate_sel_parent_daa < cutoff_daa
    }
}
#[derive(Clone)]
pub struct PochmSegment {
    pmr_witness: MerkleWitness,
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
