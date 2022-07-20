use crate::domain::consensus::model::{
    api::hash::DomainHash,
    services::reachability_service::{ReachabilityError, ReachabilityService},
    staging_area::StagingArea,
    stores::reachability_store::{ReachabilityData, ReachabilityStore},
};

pub type Result<T> = std::result::Result<T, ReachabilityError>;

pub struct ReachabilityManager {
    store: Box<dyn ReachabilityStore>,
}

impl ReachabilityManager {
    pub fn new(store: Box<dyn ReachabilityStore>) -> Self {
        Self { store }
    }

    fn add_child(
        &self, staging: &dyn StagingArea, selected_parent: &DomainHash, block: &DomainHash, reindex_root: &DomainHash,
    ) -> Result<()> {
        todo!()
    }

    fn insert_to_fcs(
        &self, staging: &dyn StagingArea, merged_block: &DomainHash, merging_block: &DomainHash,
    ) -> Result<()> {
        todo!()
    }

    fn get_reindex_root(&self, staging: &dyn StagingArea) -> Result<DomainHash> {
        todo!()
    }

    fn update_reindex_root(&self, staging: &dyn StagingArea, selected_leaf: &DomainHash) -> Result<()> {
        todo!()
    }
}

impl ReachabilityService for ReachabilityManager {
    fn init(&mut self, staging: &dyn StagingArea) -> Result<()> {
        todo!()
    }

    fn add_block(
        &mut self, staging: &dyn StagingArea, block: DomainHash, selected_parent: DomainHash, mergeset: &[DomainHash],
        is_selected_leaf: bool,
    ) -> Result<()> {
        // Allocate and stage new reachability data
        let data = ReachabilityData::new();
        self.store.stage(staging, &block, &data)?;

        // Get current reindex root
        let reindex_root = self.get_reindex_root(staging)?;

        // Add the new block
        self.add_child(staging, &selected_parent, &block, &reindex_root)?;

        // Update the future covering set for blocks in the mergeset
        for merged_block in mergeset {
            self.insert_to_fcs(staging, merged_block, &block)?;
        }

        // Update the reindex root by the new selected leaf
        if is_selected_leaf {
            self.update_reindex_root(staging, &block)?;
        }

        Ok(())
    }

    fn is_chain_ancestor_of(&self, low: DomainHash, high: DomainHash) -> Result<bool> {
        todo!()
    }

    fn is_dag_ancestor_of(&self, low: DomainHash, high: DomainHash) -> Result<bool> {
        todo!()
    }

    fn get_next_chain_ancestor(&self, descendant: DomainHash, ancestor: DomainHash) -> Result<DomainHash> {
        todo!()
    }
}
