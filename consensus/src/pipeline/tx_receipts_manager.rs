use super::receipts_errors::ReceiptsErrors;
use crate::model::stores::{
    pchmr_store::{DbPchmrStore, PchmrStoreReader},
    selected_chain::SelectedChainStoreReader,
};
use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{acceptance_data::AcceptanceDataStoreReader, headers::HeaderStoreReader, reachability::ReachabilityStoreReader},
};
use kaspa_consensus_core::{config::genesis::GenesisBlock, header::Header, receipts::{Pochm, ProofOfPublication, TxReceipt}};
use kaspa_hashes::Hash;
use kaspa_hashes::ZERO_HASH;
use kaspa_merkle::{
    calc_merkle_root, create_merkle_witness_from_sorted, create_merkle_witness_from_unsorted, verify_merkle_witness, MerkleWitness,
};

use parking_lot::RwLock;

use std::{cmp::min, sync::Arc};
#[derive(Clone)]
pub struct MerkleProofsManager<
    T: SelectedChainStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    X: AcceptanceDataStoreReader,
>
{
    pub genesis: GenesisBlock,
 
    pub posterity_depth: u64,

  
    pub headers_store: Arc<V>,
    pub  selected_chain_store: Arc<RwLock<T>>, 
   
    pub hash_to_pchmr_store: Arc<DbPchmrStore>,

  
    pub acceptance_data_store: Arc<X>,
    pub reachability_service: MTReachabilityService<U>, 
    pub storage_mass_activation_daa_score: u64,
}

impl<
        T: SelectedChainStoreReader,
        U: ReachabilityStoreReader,
        V: HeaderStoreReader,
        X: AcceptanceDataStoreReader,
    > MerkleProofsManager<T, U, V, X>
{
    pub fn new(
        genesis: GenesisBlock,
        posterity_depth: u64,
        reachability_service: MTReachabilityService<U>,
        headers_store: Arc<V>,
        selected_chain_store: Arc<RwLock<T>>,
        acceptance_data_store: Arc<X>,
        hash_to_pchmr_store: Arc<DbPchmrStore>,
        storage_mass_activation_daa_score: u64,
    ) -> Self {
        Self {
            genesis: genesis.clone(),
            posterity_depth,
            headers_store,
            selected_chain_store: selected_chain_store.clone(),
            acceptance_data_store: acceptance_data_store.clone(),
            reachability_service,
            storage_mass_activation_daa_score,
            hash_to_pchmr_store: hash_to_pchmr_store.clone(),
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
        let mut pochm_proof = Pochm::new();
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
                return verify_pchmrs_path(witness.clone(),req_block_hash,self.hash_to_pchmr_store.clone());
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
    pub fn calc_pchmr_root_by_parent(&self, req_selected_parent: Hash) -> Hash {
        /*  function receives the selected parent of the relevant block,
        as the block itself at this point is not assumed to exist*/

        let representative_parents_list = self.representative_log_parents(req_selected_parent);
        calc_merkle_root(representative_parents_list.into_iter())
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
        let mut accepted_txs= mergeset_txs_manager.iter()
            .map(|parent_acc_data|parent_acc_data.accepted_transactions
            .iter().map(|t|t.transaction_id)).flatten().collect::<Vec<Hash>>();
        accepted_txs.sort();

        create_merkle_witness_from_sorted(accepted_txs.into_iter(), tracked_tx_id).map_err(|e| e.into())
    }
    pub fn verify_merkle_witness_for_tx(&self, witness: &MerkleWitness, tracked_tx_id: Hash, req_block_hash: Hash) -> bool {
        // maybe make it return result? rethink
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
        let pre_posterity_hash = self.get_pre_posterity_block_by_parent(req_block_parent);
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
        The post posterity of a posterity block, is not the block itself rather the posterity after it
         */
        let block_bscore: u64 = self.headers_store.get_blue_score(block_hash).unwrap();
        let tentative_cutoff_bscore = block_bscore - block_bscore % self.posterity_depth + self.posterity_depth;
        let head_hash = self.selected_chain_store.read().get_tip()?.1; //possibly inefficient

        /*try and reach the first proceeding selected chain block,
        while checking if post_posterity of queried block is of the rare case where it is encountered before arriving at a chain block
        in the majority of cases, a very short distance is covered before reaching a chain block.
        */
        let candidate_block = self
            .reachability_service
            .forward_chain_iterator(block_hash, head_hash, true)
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
        let head_bscore = self.headers_store.get_blue_score(head_hash).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth + self.posterity_depth;
        if cutoff_bscore > head_bscore {
            // Posterity block not yet available.
            Err(ReceiptsErrors::PosterityDoesNotExistYet(cutoff_bscore))
        } else {
            self.get_chain_block_posterity_by_bscore(candidate_block, cutoff_bscore)
        }
    }
    pub fn get_pre_posterity_block_by_hash(&self, block_hash: Hash) -> Hash {
        if block_hash == self.genesis.hash {
            return block_hash;
        }
        let parent_hash = self.reachability_service.get_chain_parent(block_hash);
        self.get_pre_posterity_block_by_parent(parent_hash)
    }

    pub fn get_pre_posterity_block_by_parent(&self, block_parent_hash: Hash) -> Hash {
        /* the function assumes that the path from block_parent_hash down to its posterity is intact and has not been pruned
        (which should be the same as assuming block_hash has not been pruned)
        it will panic if not.
        The function does not assume block_hash is a chain block, however
        the known aplications of posterity blocks appear nonsensical when it is not
        The pre posterity of a posterity block, is not the block itself rather the posterity before it
        The posterity of genesis is defined to be genesis
        */
        let block_bscore: u64 = self.headers_store.get_blue_score(block_parent_hash).unwrap();
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
            return self.reachability_service.forward_chain_iterator(posterity_parent, block_parent_hash, true)
            .nth(1)             //skip posterity parent
            .unwrap();
        }
        //othrewise, recalculate the cutoff score in accordance to the candiate
        let candidate_bscore: u64 = self.headers_store.get_blue_score(candidate_block).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth;

        self.get_chain_block_posterity_by_bscore(candidate_block, cutoff_bscore).unwrap()
    }

    fn get_chain_block_posterity_by_bscore(
        &self,
        reference_block: Hash,
        cutoff_bscore: u64,
    ) -> Result<Hash, ReceiptsErrors> {
        //reference_block is assumed to be a chain block
        /*returns  the first posterity block with bscore smaller or equal to the cutoff bscore
        assumes data is available, will panic if not*/

        if cutoff_bscore == 0
        // edge case
        {
            return Ok(self.genesis.hash);
        }
        let mut next_candidate = reference_block;
        let mut candidate_bscore = self.headers_store.get_blue_score(next_candidate).unwrap();
        let mut candidate_index = self.selected_chain_store.read().get_by_hash(next_candidate).unwrap();

        let mut low = candidate_index.saturating_sub(self.posterity_depth);
        let mut high = min(self.selected_chain_store.read().get_tip().unwrap().0, candidate_index + self.posterity_depth);
        let mut index_step;

        let mut estimated_width =self.estimate_dag_width(); // a very rough estimation in case None was given, division by 0 averted

        if high < candidate_index {
            return Err(ReceiptsErrors::PosterityDoesNotExistYet(cutoff_bscore));
        };
        loop {
            /* a binary search 'style' loop
            with special checks to avoid getting stuck in a back and forth
            Suggestion: make recursive   */

            /*Special attention is taken to prevent a 0 value from occuring
            and causing divide by 0 or lack of progress
            estimated width is hence guaranteed a non zero value*/
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
            divide by 0 doesn't occur since index_step!=0
            Should reconsider whether this is even worth calculating iteratively compared to just the initial guess:
            very likely not*/
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
        if self.selected_chain_store.read().get_by_hash(post_posterity_candidate_hash).is_err() {
            return false;
        }
        let bscore = self.headers_store.get_blue_score(block_hash).unwrap();
        let cutoff_bscore = bscore - bscore % self.posterity_depth + self.posterity_depth;
        let candidate_sel_parent_hash = self.reachability_service.get_chain_parent(post_posterity_candidate_hash);
        let candidate_sel_parent_bscore = self.headers_store.get_blue_score(candidate_sel_parent_hash).unwrap();
        candidate_sel_parent_bscore < cutoff_bscore
    }
    pub fn estimate_dag_width(&self)->u64
    {// a  rough estimation
        let past:u64=std::cmp::min(100, self.posterity_depth);//edge case relevant for testing mostly
        let (tip_index,tip)=self.selected_chain_store.read().get_tip().unwrap();
        let tip_bscore=self.headers_store.get_blue_score(tip).unwrap();
        let past_bscore= self.headers_store
        .get_blue_score(self.selected_chain_store.read().get_by_index(tip_index.saturating_sub(past)).unwrap())
        .unwrap();
        std::cmp::max(tip_bscore.saturating_sub(past_bscore)/past,1)//avoiding a harmful 0 value
    }
}
    // this logic should be in receipts, it is only here currently because pchmr_store is required for it
    pub fn verify_pchmrs_path(pochm:Pochm, destination_block_hash: Hash,pchmr_store:Arc<DbPchmrStore>) -> bool {
        let leaf_hashes = pochm.vec.iter()
        .skip(1)//remove first element to match accordingly to witnesses 
        .map(|pochm_seg| pochm_seg.header.hash)//map to hashes
        .chain(std::iter::once(destination_block_hash)); // add final block

        /*verify the path from posterity down to req_block_hash:
        iterate downward from posterity block header: for each, check that leaf hash is  */
        for (pochm_seg, leaf_hash) in pochm.vec.iter().zip(leaf_hashes) {
            let pchmr_root_hash = pchmr_store.get(pochm_seg.header.hash).unwrap();
            let witness = &pochm_seg.leaf_in_pchmr_witness;
            if !(verify_merkle_witness(witness, leaf_hash, pchmr_root_hash)) {
                return false;
            }
        }
        true
    }