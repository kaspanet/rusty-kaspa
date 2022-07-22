use thiserror::Error;

use crate::domain::consensus::model::{
    api::hash::DomainHash, stores::errors::StoreError, stores::reachability::ReachabilityStore,
};

#[derive(Error, Debug)]
pub enum ReachabilityError {
    #[error("data store error")]
    ReachabilityStoreError(#[from] StoreError),
}

pub type Result<T> = std::result::Result<T, ReachabilityError>;

pub fn init(store: &mut dyn ReachabilityStore) -> Result<()> {
    todo!()
}

pub fn add_block(
    store: &mut dyn ReachabilityStore, block: &DomainHash, selected_parent: &DomainHash, mergeset: &[DomainHash],
    is_selected_leaf: bool,
) -> Result<()> {
    let remaining = store.remaining_interval_after(selected_parent)?;

    store.append_child(selected_parent, block)?;

    if remaining.is_empty() {
        store.insert(block, selected_parent, remaining)?;

        //
        // Start reindex context
        //
    } else {
        let allocated = remaining.split_half().0;
        store.insert(block, selected_parent, allocated)?;
    }

    // // Update the future covering set for blocks in the mergeset
    // for merged_block in mergeset {
    //     self.insert_to_fcs(store, merged_block, &block)?;
    // }

    // // Update the reindex root by the new selected leaf
    // if is_selected_leaf {
    //     self.update_reindex_root(store, &block)?;
    // }

    Ok(())
}

pub fn is_chain_ancestor_of(store: &dyn ReachabilityStore, anchor: &DomainHash, queried: &DomainHash) -> Result<bool> {
    todo!()
}

pub fn is_dag_ancestor_of(store: &dyn ReachabilityStore, anchor: &DomainHash, queried: &DomainHash) -> Result<bool> {
    todo!()
}

pub fn get_next_chain_ancestor(
    store: &dyn ReachabilityStore, descendant: &DomainHash, ancestor: &DomainHash,
) -> Result<DomainHash> {
    todo!()
}
