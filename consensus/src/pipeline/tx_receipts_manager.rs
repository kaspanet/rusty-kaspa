use super::receipts_errors::ReceiptsErrors;
use crate::model::stores::{
    block_transactions::BlockTransactionsStoreReader,
    pchmr_store::{DbPchmrStore, PchmrStoreReader},
    pruning::PruningStoreReader,
    relations::RelationsStoreReader,
    selected_chain::SelectedChainStoreReader,
};
use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{acceptance_data::AcceptanceDataStoreReader, headers::HeaderStoreReader, reachability::ReachabilityStoreReader},
};
use kaspa_consensus_core::{
    config::{genesis::GenesisBlock, params::ForkActivation},
    hashing,
    header::Header,
    merkle::create_hash_merkle_witness,
    receipts::{Pochm, ProofOfPublication, TxReceipt},
};
use kaspa_hashes::Hash;
use kaspa_hashes::ZERO_HASH;
use kaspa_merkle::{
    calc_merkle_root, create_merkle_witness_from_sorted, create_merkle_witness_from_unsorted, verify_merkle_witness, MerkleWitness,
};

use parking_lot::RwLock;

use std::{
    cmp::min,
    collections::{HashSet, VecDeque},
    sync::Arc,
};
#[derive(Clone)]
pub struct TxReceiptsManager<
    T: SelectedChainStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    X: AcceptanceDataStoreReader,
    W: BlockTransactionsStoreReader,
    Y: PruningStoreReader,
    I: RelationsStoreReader,
> {
    pub genesis: GenesisBlock,

    pub posterity_depth: u64,
    pub reachability_service: MTReachabilityService<U>,

    pub headers_store: Arc<V>,
    pub selected_chain_store: Arc<RwLock<T>>,
    pub acceptance_data_store: Arc<X>,
    pub block_transactions_store: Arc<W>,
    pub relations_store: Arc<RwLock<Vec<I>>>,

    pub hash_to_pchmr_store: Arc<DbPchmrStore>,
    pub pruning_point_store: Arc<RwLock<Y>>,

    pub storage_mass_activation: ForkActivation,
}

impl<
        T: SelectedChainStoreReader,
        U: ReachabilityStoreReader,
        V: HeaderStoreReader,
        X: AcceptanceDataStoreReader,
        W: BlockTransactionsStoreReader,
        Y: PruningStoreReader,
        I: RelationsStoreReader,
    > TxReceiptsManager<T, U, V, X, W, Y, I>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        genesis: GenesisBlock,
        posterity_depth: u64,
        reachability_service: MTReachabilityService<U>,
        headers_store: Arc<V>,
        selected_chain_store: Arc<RwLock<T>>,
        acceptance_data_store: Arc<X>,
        block_transactions_store: Arc<W>,
        pruning_point_store: Arc<RwLock<Y>>,
        relations_store: Arc<RwLock<Vec<I>>>,

        hash_to_pchmr_store: Arc<DbPchmrStore>,
        storage_mass_activation: ForkActivation,
    ) -> Self {
        Self {
            genesis: genesis.clone(),
            posterity_depth,
            headers_store,
            selected_chain_store: selected_chain_store.clone(),
            acceptance_data_store: acceptance_data_store.clone(),
            reachability_service,
            storage_mass_activation,
            hash_to_pchmr_store: hash_to_pchmr_store.clone(),
            block_transactions_store: block_transactions_store.clone(),
            pruning_point_store: pruning_point_store.clone(),
            relations_store: relations_store.clone(),
        }
    }
    pub fn generate_tx_receipt(&self, accepting_block_header: Arc<Header>, tracked_tx_id: Hash) -> Result<TxReceipt, ReceiptsErrors> {
        let pochm = self.create_pochm_proof(accepting_block_header.hash)?;
        //find the accepted tx in accepting_block_hash and create a merkle witness for it
        let mergeset_txs_manager = self.acceptance_data_store.get(accepting_block_header.hash)?;
        let mut accepted_txs = mergeset_txs_manager
            .iter()
            .flat_map(|parent_acc_data| parent_acc_data.accepted_transactions.iter().map(|t| t.transaction_id))
            .collect::<Vec<Hash>>();
        accepted_txs.sort();

        let tx_acc_proof = create_merkle_witness_from_sorted(accepted_txs.into_iter(), tracked_tx_id)?;

        Ok(TxReceipt { tracked_tx_id, accepting_block_header, pochm, tx_acc_proof })
    }
    pub fn generate_proof_of_pub(
        &self,
        pub_block_header: Arc<Header>,
        tracked_tx_id: Hash,
    ) -> Result<ProofOfPublication, ReceiptsErrors> {
        let path_to_selected = self.find_future_chain_block_path(pub_block_header.hash)?;

        let mut headers_path_to_selected: Vec<_> =
            path_to_selected.iter().map(|&hash| self.headers_store.get_header(hash).unwrap()).collect();

        let pochm = self.create_pochm_proof(headers_path_to_selected.last().unwrap().hash)?;
        headers_path_to_selected.remove(0); //remove the publishing block itself from the chain as it is redundant to store

        //next, find the relevant transaction in pub_block_hash's published transactions and create a merkle witness for it
        let published_txs = self.block_transactions_store.get(pub_block_header.hash)?;
        let tracked_tx = published_txs.iter().find(|tx| tx.id() == tracked_tx_id).unwrap();
        let include_mass_field = self.storage_mass_activation.is_active(pub_block_header.daa_score);
        let tx_pub_proof = create_hash_merkle_witness(published_txs.iter(), tracked_tx, include_mass_field)?;

        let tracked_tx_hash = hashing::tx::hash(tracked_tx, include_mass_field); //leaf value in merkle tree
        Ok(ProofOfPublication { tracked_tx_hash, pub_block_header, pochm, tx_pub_proof, headers_path_to_selected })
    }
    pub fn verify_tx_receipt(&self, tx_receipt: &TxReceipt) -> bool {
        let acc_atmr = tx_receipt.accepting_block_header.accepted_id_merkle_root;
        verify_merkle_witness(&tx_receipt.tx_acc_proof, tx_receipt.tracked_tx_id, acc_atmr)
            && self.verify_pochm_proof(tx_receipt.accepting_block_header.hash, &tx_receipt.pochm)
    }
    pub fn verify_proof_of_pub(&self, proof_of_pub: &ProofOfPublication) -> bool {
        let valid_path = proof_of_pub
            .headers_path_to_selected
            .iter()
            .try_fold(
                proof_of_pub.pub_block_header.hash,
                |curr, next| if next.direct_parents().contains(&curr) { Some(next.hash) } else { None },
            )
            .is_some();
        if !valid_path {
            return false;
        };
        let earliest_selected_chain_decendant =
            proof_of_pub.headers_path_to_selected.last().unwrap_or(&proof_of_pub.pub_block_header).hash;
        let pub_merkle_root = proof_of_pub.pub_block_header.hash_merkle_root;
        verify_merkle_witness(&proof_of_pub.tx_pub_proof, proof_of_pub.tracked_tx_hash, pub_merkle_root)
            && self.verify_pochm_proof(earliest_selected_chain_decendant, &proof_of_pub.pochm)
    }

    /*Assumes: chain_purporter is on the selected chain,
    if not returns error   */
    pub fn create_pochm_proof(&self, chain_purporter: Hash) -> Result<Pochm, ReceiptsErrors> {
        let mut pochm_proof = Pochm::new();
        let purporter_index = self
            .selected_chain_store
            .read()
            .get_by_hash(chain_purporter)
            .map_err(|_| ReceiptsErrors::RequestedBlockNotOnSelectedChain(chain_purporter))?;
        let post_posterity_hash = self.get_post_posterity_block(chain_purporter)?;

        /*
           iterate from post posterity down  to the chain purpoter creating pchmr witnesses along the way
        */
        let mut leaf_block_index;
        let mut leaf_block_hash;

        let posterity_index = self.selected_chain_store.read().get_by_hash(post_posterity_hash).unwrap();
        let (mut root_block_hash, mut root_block_index) = (post_posterity_hash, posterity_index); //first root is post posterity
        let mut remaining_index_diff = root_block_index - purporter_index;
        while remaining_index_diff > 0 {
            leaf_block_index = root_block_index - (remaining_index_diff + 1).next_power_of_two() / 2; //subtract highest possible power of two such as to not cross 0
            leaf_block_hash = self.selected_chain_store.read().get_by_index(leaf_block_index)?;

            let leaf_is_in_pchmr_of_root_proof = self.create_pchmr_witness(leaf_block_hash, root_block_hash)?;
            let root_block_header = self.headers_store.get_header(root_block_hash).unwrap();
            pochm_proof.insert(root_block_header.clone(), leaf_is_in_pchmr_of_root_proof);

            (root_block_hash, root_block_index) = (leaf_block_hash, leaf_block_index);
            remaining_index_diff = root_block_index - purporter_index;
        }
        Ok(pochm_proof)
    }

    /*this function will return true for any witness premiering with a currently non pruned block and
    recursively pointing down to chain_purporter, it is the responsibility of the
    creator of the witness to make sure the witness premiers with a posterity block
    and not just any block that may be pruned in the future, as this property is not verified in this function,
    and the function should not be relied upon to confirm the witness is everlasting*/
    pub fn verify_pochm_proof(&self, chain_purporter: Hash, witness: &Pochm) -> bool {
        if let Some(post_posterity_hash) = witness.get_path_origin() {
            if self.headers_store.get_header(post_posterity_hash).is_ok()
            // verify the corresponding header is available
            {
                //verification of path itself is delegated to the pochm struct
                return verify_pchmrs_path(witness, chain_purporter, self.hash_to_pchmr_store.clone());
            }
        }
        false
    }
    pub fn calc_pchmr_root_by_hash(&self, block_hash: Hash) -> Hash {
        if block_hash == self.genesis.hash {
            return ZERO_HASH;
        }
        let parent = self.reachability_service.get_chain_parent(block_hash);
        self.calc_pchmr_root_by_parent(parent)
    }

    /*  function receives the selected parent of the relevant block,
    as the block itself at this point is not assumed to exist*/
    pub fn calc_pchmr_root_by_parent(&self, parent_of_queried_block: Hash) -> Hash {
        let representative_parents_list = self.representative_log_parents(parent_of_queried_block);
        calc_merkle_root(representative_parents_list.into_iter())
    }

    /*proof that a block belongs to the pchmr tree of another block;
    the function assumes that the path from block_hash down to its posterity is intact and has not been pruned
    (which should be the same as assuming block_hash has not been pruned)
    it will panic if not.*/
    pub fn create_pchmr_witness(&self, leaf_block_hash: Hash, root_block_hash: Hash) -> Result<MerkleWitness, ReceiptsErrors> {
        let parent_of_root = self.reachability_service.get_chain_parent(root_block_hash);
        let log_sized_parents_list = self.representative_log_parents(parent_of_root);

        create_merkle_witness_from_unsorted(log_sized_parents_list.into_iter(), leaf_block_hash).map_err(|e| e.into())
    }
    pub fn verify_pchmr_witness(&self, witness: &MerkleWitness, leaf_block_hash: Hash, root_block_hash: Hash) -> bool {
        verify_merkle_witness(witness, leaf_block_hash, self.hash_to_pchmr_store.get(root_block_hash).unwrap())
    }

    /* the function assumes that the path from block_hash down to its posterity is intact and has not been pruned
    (which should be the same as assuming block_hash has not been pruned)
    it will panic if not.
    Function receives the selected parent of the relevant block, as the block itself is not assumed to necessarily exist yet
    Returns all 2^i deep 'selected' parents up to the posterity block not included */
    fn representative_log_parents(&self, parent_of_queried_block: Hash) -> Vec<Hash> {
        let pre_posterity_hash = self.get_pre_posterity_block_by_parent(parent_of_queried_block);
        let pre_posterity_bscore = self.headers_store.get_blue_score(pre_posterity_hash).unwrap();
        let mut representative_parents_list = vec![];
        /*The following logic will not be efficient for blocks which are a long distance away from the selected chain,
        Hence, the corresponding field for which this calculation should only be verified for selected chain candidates
        This function is also called when creating said field - in this case however any honest node should only call for it on a block which
        would be on the selected chain from its point of view
        nethertheless, the logic will always return a correct answer if called.*/
        let mut distance_covered_before_chain = 0; //compulsory initialization for compiler only , a chain block will have to be reached eventually
        let mut first_chain_ancestor = ZERO_HASH; //compulsory initialization for compiler only, a chain block will have to be reached eventually
        for (i, current) in
            self.reachability_service.backward_chain_iterator(parent_of_queried_block, self.genesis.hash, true).enumerate()
        {
            let index = i + 1; //enumeration should start from 1
            if self.selected_chain_store.read().get_by_hash(current).is_ok() {
                // get out of loop and apply selected chain logic instead
                first_chain_ancestor = current;
                distance_covered_before_chain = index as u64;
                break;
            } else if index.is_power_of_two() {
                //trickery to check if index is a power of two
                representative_parents_list.push(current);
            }
            if current == pre_posterity_hash {
                // notice the pre_posterity for a non chain block is not necessarily a chain block
                return representative_parents_list;
            }
        }
        let first_chain_ancestor_index = self.selected_chain_store.read().get_by_hash(first_chain_ancestor).unwrap();
        representative_parents_list.append(&mut self.representative_log_parents_from_selected_chain(
            first_chain_ancestor_index,
            distance_covered_before_chain,
            pre_posterity_bscore,
        ));
        representative_parents_list
    }

    /*
       get the representative parents of a (implicit) queried block,
       who are also ancestors of  the firs ancestor of queried block which is in the selected chain.
       distance_covered_before_chain describes the  distance from queried block to  its first chain ancestor
       Be wary this is only a partial list of all representative parents of queried block
    */
    fn representative_log_parents_from_selected_chain(
        &self,
        first_chain_ancestor_index: u64,
        distance_covered_before_chain: u64,
        pre_posterity_bscore: u64,
    ) -> Vec<Hash> {
        let mut representative_parents_partial_list = vec![];
        let queried_block_fictional_index = first_chain_ancestor_index + distance_covered_before_chain;
        let mut next_power = distance_covered_before_chain.next_power_of_two();
        let mut next_chain_block_rep_parent =
            self.selected_chain_store.read().get_by_index(queried_block_fictional_index.saturating_sub(next_power)).unwrap();
        let mut next_bscore = self.headers_store.get_blue_score(next_chain_block_rep_parent).unwrap();
        while next_bscore > pre_posterity_bscore {
            representative_parents_partial_list.push(next_chain_block_rep_parent);
            next_power *= 2;
            if let Ok(unwarapped) = self
                .selected_chain_store
                .read()
                .get_by_index(first_chain_ancestor_index.saturating_sub(next_power.saturating_sub(distance_covered_before_chain)))
            {
                next_chain_block_rep_parent = unwarapped;
                next_bscore = self.headers_store.get_blue_score(next_chain_block_rep_parent).unwrap();
            } else {
                break;
            }
        }
        if next_bscore == pre_posterity_bscore {
            //edge case
            representative_parents_partial_list.push(next_chain_block_rep_parent);
        }
        representative_parents_partial_list
    }
    /* the function assumes that the path from block_hash up to its post posterity if it exits is intact and has not been pruned
    (which should be the same as assuming block_hash has not been pruned)
    it will panic if not;
    An error is returned if post_posterity does not yet exist;
    The function does not assume block_hash is a chain block, however
    the known aplications of posterity blocks appear nonsensical when it is not;
    The post posterity of a posterity block, is not the block itself rather the posterity after it. */
    pub fn get_post_posterity_block(&self, block_hash: Hash) -> Result<Hash, ReceiptsErrors> {
        /*try and reach the first proceeding selected chain block,
        in the majority of cases, a very short distance is covered before reaching a chain block.*/

        let candidate_block = *self
            .find_future_chain_block_path(block_hash)
            .map_err(|_| ReceiptsErrors::PosterityDoesNotExistYet(block_hash))?
            .last()
            .unwrap();
        let candidate_bscore = self.headers_store.get_blue_score(candidate_block).unwrap();

        let head_hash = self.selected_chain_store.read().get_tip()?.1;
        let head_bscore = self.headers_store.get_blue_score(head_hash).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth + self.posterity_depth;
        if cutoff_bscore > head_bscore {
            // Posterity block not yet available.
            Err(ReceiptsErrors::PosterityDoesNotExistYet(block_hash))
        } else {
            Ok(self.get_chain_block_by_cutoff_bscore(candidate_block, cutoff_bscore, self.posterity_depth))
        }
    }
    pub fn get_pre_posterity_block_by_hash(&self, block_hash: Hash) -> Hash {
        if block_hash == self.genesis.hash {
            return block_hash;
        }
        let parent_hash = self.reachability_service.get_chain_parent(block_hash);
        self.get_pre_posterity_block_by_parent(parent_hash)
    }

    /* the function assumes that the path from block_parent_hash down to its posterity is intact and has not been pruned
    (which should be the same as assuming block_hash has not been pruned)
    it will panic if not.
    The function does not assume block_hash is a chain block, however
    the known aplications of posterity blocks appear nonsensical when it is not
    The pre posterity of a posterity block, is not the block itself rather the posterity before it
    The posterity of genesis is defined to be genesis*/
    pub fn get_pre_posterity_block_by_parent(&self, block_parent_hash: Hash) -> Hash {
        let block_bscore = self.headers_store.get_blue_score(block_parent_hash).unwrap();
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
            .default_backward_chain_iterator(block_parent_hash)
            .find(|&block| {
                self.headers_store.get_blue_score(block).unwrap() < tentative_cutoff_bscore
                    || self.selected_chain_store.read().get_by_hash(block).is_ok()
            })
            .unwrap();
        // in case cutoff_bscore was crossed prior to reaching a chain block
        if self.headers_store.get_blue_score(candidate_block).unwrap() < tentative_cutoff_bscore {
            let posterity_parent = candidate_block;
            return self.reachability_service.get_next_chain_ancestor(block_parent_hash, posterity_parent);
        }
        //othrewise, recalculate the cutoff score in accordance to the candiate
        let candidate_bscore = self.headers_store.get_blue_score(candidate_block).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth;

        self.get_chain_block_by_cutoff_bscore(candidate_block, cutoff_bscore, self.posterity_depth)
    }

    /*returns the first chain block with bscore larger or equal to the cutoff bscore;
    assumes data is available, will panic if not;
    reference_block is assumed to be a chain block;
    strongly assumes the required chain block is no more than max_distance away from refernce,
    and that the tip has higher bscore than the cutoff - the caller is responsible for this guarantee.
    */
    fn get_chain_block_by_cutoff_bscore(&self, reference_block: Hash, cutoff_bscore: u64, max_distance: u64) -> Hash {
        if cutoff_bscore == 0
        // edge case
        {
            return self.genesis.hash;
        }
        let reference_header = self.headers_store.get_header(reference_block).unwrap();
        let reference_index = self.selected_chain_store.read().get_by_hash(reference_block).unwrap();

        let pruning_read = self.pruning_point_store.read(); //should I keep this lock till the end?
        let pruning_point_index = self.selected_chain_store.read().get_by_hash(pruning_read.pruning_point().unwrap()).unwrap();
        drop(pruning_read);

        let low = std::cmp::max(reference_index.saturating_sub(max_distance), pruning_point_index);
        let high = min(self.selected_chain_store.read().get_tip().unwrap().0, reference_index + max_distance);
        let estimated_width = self.estimate_dag_width(None); //rough initial estimation

        assert!(reference_index <= high);

        self.get_chain_block_by_cutoff_bscore_rec(cutoff_bscore, low, high, reference_header, estimated_width)
    }

    /* a binary search 'style' recursive function
    with special checks in place to avoid getting stuck in a back and forth
    notice the function gets a header and not a hash
    candidate_header is assumed to be a chain block
    */
    fn get_chain_block_by_cutoff_bscore_rec(
        &self,
        cutoff_bscore: u64,
        mut low: u64,
        mut high: u64,
        candidate_header: Arc<Header>,
        estimated_width: u64,
    ) -> Hash {
        let candidate_index = self.selected_chain_store.read().get_by_hash(candidate_header.hash).unwrap();
        let next_candidate_index;
        /*Special attention is taken to prevent a 0 value from occuring
        and causing divide by 0 or lack of progress
        estimated width is hence guaranteed a non zero value*/
        let mut index_step = (candidate_header.blue_score.abs_diff(cutoff_bscore)) / estimated_width;
        index_step = if index_step == 0 { 1 } else { index_step };
        if candidate_header.blue_score < cutoff_bscore {
            // in this case index should move forward
            if low < candidate_index
            //rescale bound and update
            {
                low = candidate_index; //rescale bound
                next_candidate_index = candidate_index + index_step;
                eprintln!("forward:{}", next_candidate_index);
            } else {
                /*  if candidate slipped outside the already known bounds,
                we risk making no progress and  getting stuck inside a back and forth loop,
                so just iterate forwards on route to high_block till cutoff is found.*/
                return self.get_chain_block_by_cutoff_bscore_linearly_forwards(candidate_header, high, cutoff_bscore);
            }
        } else {
            // in this case index will move backward
            if high > candidate_index {
                let candidate_parent = self.reachability_service.get_chain_parent(candidate_header.hash);
                let candidate_parent_bscore = self.headers_store.get_blue_score(candidate_parent).unwrap();
                if candidate_parent_bscore < cutoff_bscore
                // first check if candidate actually is the cutoff block
                {
                    return candidate_header.hash;
                } else {
                    // if not, update candidate indices and bounds
                    high = candidate_index; //  rescale bound
                    next_candidate_index = candidate_index.saturating_sub(index_step);
                    eprintln!("backward:{}", next_candidate_index);

                    //shouldn't overflow in natural conditions but does in testing
                }
            } else {
                /*again avoid getting stuck in a back and forth loop
                by iterating backwards down to low */
                return self.get_chain_block_by_cutoff_bscore_linearly_backwards(candidate_header, low, cutoff_bscore);
            }
        }
        let next_candidate_hash = self.selected_chain_store.read().get_by_index(next_candidate_index);
        if next_candidate_hash.is_err() {
            //forced repetition if index out of bounds...
            // panic!();
            return self.get_chain_block_by_cutoff_bscore_rec(cutoff_bscore, low, high, candidate_header, estimated_width);
        }
        let next_candidate_hash = next_candidate_hash.unwrap();
        let next_candidate_header = self.headers_store.get_header(next_candidate_hash).unwrap();

        /*Update the estimated width based on the latest result;
        Notice a 0 value can never occur:
        A) because index_step!=0, meaning next_candidate_bscore and candidate_bscore are strictly different
        B) because |next_candidate_bscore-candidate_bscore| is by definition the minimal possible value index_step can get
        divide by 0 doesn't occur since index_step!=0;
        Should reconsider whether this logic is even worth calculating iteratively compared to just the initial guess:
        This is probably a classic premature optimization is the root of all evil.
        The scenario where I believe this iterative guessing is of use is when an archival node
        attempts to find a deep posterity block where thhe width may have flactuated a lot*/
        let next_estimated_width = (candidate_header.blue_score.abs_diff(next_candidate_header.blue_score)) / index_step;
        assert_ne!(next_estimated_width, 0);

        self.get_chain_block_by_cutoff_bscore_rec(cutoff_bscore, low, high, next_candidate_header, next_estimated_width)
    }

    fn get_chain_block_by_cutoff_bscore_linearly_backwards(&self, initial: Arc<Header>, low: u64, cutoff_bscore: u64) -> Hash {
        //initial has blue score higher than cutoff score

        let low_block = self.selected_chain_store.read().get_by_index(low).unwrap();
        //iterate back until a block is found with blue score lower than the cutoff
        let candidate_parent = self
            .reachability_service
            .backward_chain_iterator(initial.hash, low_block, true)
            .find(|&block| self.headers_store.get_blue_score(block).unwrap() < cutoff_bscore);

        if let Some(candidate_parent) = candidate_parent {
            // and then return its 'selected' son
            self.reachability_service.get_next_chain_ancestor(initial.hash, candidate_parent)
        } else {
            // if no block is found, then the block must be low_block itself.
            // We cannot explicitely check this as low_block may not have parents stored
            low_block
        }
    }
    fn get_chain_block_by_cutoff_bscore_linearly_forwards(&self, initial: Arc<Header>, high: u64, cutoff_bscore: u64) -> Hash {
        //initial has blue score less than cutoff score
        let high_block = self.selected_chain_store.read().get_by_index(high).unwrap();
        self
            .reachability_service
            .forward_chain_iterator(initial.hash, high_block, true)
            .skip(1)//we already know initial has a lower score
            .find(|&block| self.headers_store.get_blue_score(block).unwrap() >= cutoff_bscore)
            .unwrap()
    }

    // a rough estimation of the dag width
    // reference_wrapped is assumed to be a block on the selected chain, or None
    // will panic if not
    pub fn estimate_dag_width(&self, reference_wrapped: Option<Hash>) -> u64 {
        let (reference_index, reference);
        let pruning_read = self.pruning_point_store.read(); //should I keep this lock till the end?
        let pruning_point_index = self.selected_chain_store.read().get_by_hash(pruning_read.pruning_point().unwrap()).unwrap();
        drop(pruning_read);
        let past_dist_cover = std::cmp::min(100, self.posterity_depth); //edge case relevant for testing mostly
        if reference_wrapped.is_some() {
            reference = reference_wrapped.unwrap();
            reference_index = self.selected_chain_store.read().get_by_hash(reference).unwrap();
        } else {
            // if no refernce provided, take the sink as reference
            (reference_index, reference) = self.selected_chain_store.read().get_tip().unwrap();
        }
        let reference_bscore = self.headers_store.get_blue_score(reference).unwrap();
        let past_bscore = self
            .headers_store
            .get_blue_score(
                self.selected_chain_store
                    .read()
                    .get_by_index(std::cmp::max(reference_index.saturating_sub(past_dist_cover), pruning_point_index))
                    .unwrap(),
            )
            .unwrap();
        std::cmp::max(reference_bscore.saturating_sub(past_bscore) / past_dist_cover, 1)
        //avoiding a harmful 0 value
    }

    /*block hash must be an ancestor of sink, otherwise an error will be returned
    Maybe this kind of code needs to be on the traversal manager or something of sorts
    */
    pub fn find_future_chain_block_path(&self, block_hash: Hash) -> Result<Vec<Hash>, ReceiptsErrors> {
        let sink = self.selected_chain_store.read().get_tip()?.1;
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(vec![block_hash]);
        visited.insert(block_hash);
        // a standard BFS loop until a chain block is found
        while let Some(path) = queue.pop_front() {
            let curr = *path.last().unwrap(); // path should never be empty
            if !self.reachability_service.is_dag_ancestor_of(curr, sink) {
                continue;
            }

            if self.selected_chain_store.read().get_by_hash(curr).is_ok() {
                return Ok(path);
            }
            let children = self.relations_store.read()[0].get_children(curr);
            if let Ok(children) = children {
                for &child in children.read().iter() {
                    if !visited.contains(&child) {
                        let mut new_path = path.clone();
                        new_path.push(child);
                        queue.push_back(new_path);
                        visited.insert(child);
                    }
                }
            }
        }
        Err(ReceiptsErrors::NoChainBlockInFuture(block_hash))
    }
    /*the verification consists of 3 parts:
    1) verify the block queried is an ancesstor of the candidate
    2)verify the candidate is on the selected chain
    3) verify the selected parent of the candidate has blue score score smaller than the posterity designated blue score
    function hence assumes the selected parent has not been pruned;
    Currently function is not used outside of tests, arguable if it should exist.
    */
    pub fn verify_post_posterity_block(&self, block_hash: Hash, post_posterity_candidate_hash: Hash) -> bool {
        if !self.reachability_service.is_dag_ancestor_of(block_hash, post_posterity_candidate_hash) {
            return false;
        }
        if self.selected_chain_store.read().get_by_hash(post_posterity_candidate_hash).is_err() {
            return false;
        }
        let bscore = self.headers_store.get_blue_score(block_hash).unwrap();
        let cutoff_bscore = bscore - bscore % self.posterity_depth + self.posterity_depth;
        let candidate_sel_parent_hash = self.reachability_service.get_chain_parent(post_posterity_candidate_hash);
        let candidate_sel_parent_bscore = self.headers_store.get_blue_score(candidate_sel_parent_hash).unwrap();
        candidate_sel_parent_bscore < cutoff_bscore
    }
}

// this logic should be a method of Pochm in receipts.rs,
//it is only here currently because pchmr_store is required for it
// and cannot be accessed easily from receipt.rs
pub fn verify_pchmrs_path(pochm: &Pochm, destination_block_hash: Hash, pchmr_store: Arc<DbPchmrStore>) -> bool {
    let leaf_hashes = pochm.vec.iter()
        .skip(1)//remove first element to match accordingly to witnesses 
        .map(|pochm_seg| pochm_seg.header.hash)//map to hashes
        .chain(std::iter::once(destination_block_hash)); // add final block

    /*verify the path from posterity down to chain_purporter:
    iterate downward from posterity block header: for each, verify that leaf hash is in pchmr of ther header */
    pochm.vec.iter().zip(leaf_hashes).all(|(pochm_seg, leaf_hash)| {
        let pchmr_root_hash = pchmr_store.get(pochm_seg.header.hash).unwrap();
        let witness = &pochm_seg.leaf_in_pchmr_witness;
        verify_merkle_witness(witness, leaf_hash, pchmr_root_hash)
    })
}
