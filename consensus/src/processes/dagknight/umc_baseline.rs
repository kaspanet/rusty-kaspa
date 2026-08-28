use std::{collections::HashMap, sync::Arc};

use kaspa_consensus_core::BlueWorkType;
use kaspa_hashes::Hash;
use num_traits::Zero;

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{headers::HeaderStoreReader, reachability::ReachabilityStoreReader},
    },
    processes::{
        dagknight::{
            Bucket,
            umc_voting::{CascadeResult, SignedWork, UmcVoter, UmcVotingContext},
        },
        difficulty::calc_work,
        ghostdag::ordering::SortableBlock,
    },
};

/// Baseline UMC cascade voter: naive reference implementation of paper Algorithm 6
/// (work-weighted). Each blue's vote is recomputed from scratch on every call:
///
///   vote(B) = sign(Σ vote(blue ∈ future(B)) - red_work(future(B)) + deficit) * work(B)
///
/// Grays (red blocks that have the subgroup's next chain ancestor as a chain ancestor)
/// do not vote.
pub struct BaselineUmcVoter<O: HeaderStoreReader + 'static, R: ReachabilityStoreReader + Clone> {
    headers_store: Arc<O>,
    reachability_service: MTReachabilityService<R>,
}

impl<O: HeaderStoreReader + 'static, R: ReachabilityStoreReader + Clone> BaselineUmcVoter<O, R> {
    pub fn new(headers_store: Arc<O>, reachability_service: MTReachabilityService<R>) -> Self {
        Self { headers_store, reachability_service }
    }
}

impl<O: HeaderStoreReader + 'static, R: ReachabilityStoreReader + Clone> UmcVoter for BaselineUmcVoter<O, R> {
    fn vote(&self, ctx: &UmcVotingContext<'_>) -> CascadeResult {
        let conflict_genesis = ctx.conflict_genesis;
        let subgroup_member = ctx.subgroup_member;
        let virtual_gd = ctx.virtual_gd;
        let k = ctx.k;
        let coloring_reader = ctx.coloring_reader;

        let next_chain_ancestor_of_subgroup = self.reachability_service.get_next_chain_ancestor(*subgroup_member, conflict_genesis);

        let mut blues = Vec::new();
        let mut reds = Vec::new();

        let mut curr_gd = Arc::new(virtual_gd.clone());
        while curr_gd.selected_parent != conflict_genesis {
            for &blue_block in curr_gd.mergeset_blues.iter() {
                let blue_work = calc_work(self.headers_store.get_bits(blue_block).unwrap());
                let header_work = self.headers_store.get_header(blue_block).unwrap().blue_work;
                blues.push((blue_block, blue_work, header_work));
            }
            for &red_block in curr_gd.mergeset_reds.iter() {
                let red_work = calc_work(self.headers_store.get_bits(red_block).unwrap());
                let header_work = self.headers_store.get_header(red_block).unwrap().blue_work;
                if !self.reachability_service.is_chain_ancestor_of(next_chain_ancestor_of_subgroup, red_block) {
                    reds.push((red_block, red_work, header_work));
                }
            }
            curr_gd = coloring_reader.get_coloring_data(curr_gd.selected_parent);
        }

        let cg_block_work = calc_work(self.headers_store.get_bits(conflict_genesis).unwrap());
        blues.push((conflict_genesis, cg_block_work, 0.into()));

        // Deficit work = sqrt(k) * conflict_genesis_work
        let deficit_work =
            BlueWorkType::from_u64(u64::from(k.isqrt())) * calc_work(self.headers_store.get_bits(conflict_genesis).unwrap());

        let voting_blocks = (blues.len() + reds.len()) as u64;
        let is_ancestor = |a: Hash, b: Hash| self.reachability_service.is_dag_ancestor_of(a, b);
        let helper = BaselineCascadeHelper::new(blues, reds, deficit_work, &is_ancestor, conflict_genesis);
        let (total_vote, virtual_score, _per_blue_buckets) = helper.compute_all_votes();

        CascadeResult {
            virtual_score,
            accepted: total_vote >= SignedWork::zero(),
            flips: 0,
            voting_blocks,
            from_checkpoint: false,
            estimated_effort_saved: 0,
            estimated_effort_total: 0,
        }
    }
}

/// Baseline cascade: iterate blues topologically (tips→past) using a heap of SortableBlock.
/// vote(B) = sign(Σ vote(blue ∈ future(B)) - red_work(future(B)) + deficit) * work(B)
struct BaselineCascadeHelper<'a> {
    blues: Vec<(Hash, BlueWorkType, BlueWorkType)>,
    reds: Vec<(Hash, BlueWorkType, BlueWorkType)>,
    deficit: BlueWorkType,
    is_ancestor: &'a dyn Fn(Hash, Hash) -> bool,
    conflict_genesis: Hash,
}

impl<'a> BaselineCascadeHelper<'a> {
    fn new(
        blues: Vec<(Hash, BlueWorkType, BlueWorkType)>,
        reds: Vec<(Hash, BlueWorkType, BlueWorkType)>,
        deficit: BlueWorkType,
        is_ancestor: &'a dyn Fn(Hash, Hash) -> bool,
        conflict_genesis: Hash,
    ) -> Self {
        Self { blues, reds, deficit, is_ancestor, conflict_genesis }
    }

    fn compute_all_votes(&self) -> (SignedWork, SignedWork, HashMap<Hash, Bucket>) {
        use std::collections::BinaryHeap;

        let mut votes: HashMap<Hash, SignedWork> = HashMap::new();
        let mut block_work_map: HashMap<Hash, BlueWorkType> = HashMap::new();
        let mut heap: BinaryHeap<SortableBlock> = self
            .blues
            .iter()
            .map(|(hash, block_work, header_work)| {
                block_work_map.insert(*hash, *block_work);
                SortableBlock { hash: *hash, blue_work: *header_work }
            })
            .collect();

        while let Some(SortableBlock { hash: bh, .. }) = heap.pop() {
            let blue_work_signed = SignedWork::from(block_work_map[&bh]);

            // Only blues already processed (higher blue_work) can be in future of bh
            let future_blue_votes: SignedWork = votes
                .iter()
                .filter(|(other, _)| (self.is_ancestor)(bh, **other))
                .map(|(_, v)| *v)
                .fold(SignedWork::zero(), |acc, v| acc + v);

            let future_red_work: BlueWorkType =
                self.reds.iter().filter(|&&(rh, _, _)| (self.is_ancestor)(bh, rh)).map(|&(_, rw, _)| rw).sum();

            // score = future_blue_votes - future_red_work + deficit
            let score: SignedWork = future_blue_votes - SignedWork::from(future_red_work) + SignedWork::from(self.deficit);
            let v = if score >= SignedWork::zero() { blue_work_signed } else { SignedWork::zero() - blue_work_signed };

            votes.insert(bh, v);
        }

        // Convert votes to buckets for each blue based on vote sign.
        // CG's bucket is determined by its actual vote (same as all other blues).
        let per_blue_buckets: HashMap<Hash, Bucket> = votes
            .iter()
            .map(|(hash, vote)| {
                let bucket = if *vote >= SignedWork::zero() { Bucket::Positive } else { Bucket::Negative };
                (*hash, bucket)
            })
            .collect();

        let signed_blue_work = votes.values().copied().fold(SignedWork::zero(), |total, vote| total + vote);
        let red_work: BlueWorkType = self.reds.iter().map(|&(_, work, _)| work).sum();
        let virtual_score = signed_blue_work + SignedWork::from(self.deficit) - SignedWork::from(red_work);

        (votes[&self.conflict_genesis], virtual_score, per_blue_buckets)
    }
}

#[cfg(test)]
mod tests {
    use super::BaselineUmcVoter;
    use crate::processes::dagknight::umc_voting::{UmcVoter, test_fixtures::Fixture};

    #[test]
    fn test_baseline_voter_vote() {
        let fixture = Fixture::new();
        let voter = BaselineUmcVoter::new(fixture.headers.clone(), fixture.reachability.clone());
        let ctx = fixture.context();

        let result = voter.vote(&ctx);

        assert_eq!(result.virtual_score, fixture.expected_score(), "virtual score mismatch");
        assert!(result.accepted, "zone should be accepted");
        assert_eq!(result.voting_blocks, 16, "blues 11, 10, 9, 7, 6, 5, 4, 3, 2 + CG + reds 12..17 (gray red 8 excluded)");
    }
}
