use thiserror::Error;

use crate::domain::consensus::model::{
    api::hash::DomainHash,
    stores::{errors::StoreError, reachability_store::ReachabilityStore},
};

#[derive(Error, Debug)]
pub enum ReachabilityError {
    #[error("data store error")]
    ReachabilityStoreError(#[from] StoreError),
}

pub trait ReachabilityService {
    fn init(&mut self, store: &dyn ReachabilityStore) -> Result<(), ReachabilityError>;
    fn add_block(
        &mut self, store: &dyn ReachabilityStore, block: DomainHash, selected_parent: DomainHash,
        mergeset: &[DomainHash], is_selected_leaf: bool,
    ) -> Result<(), ReachabilityError>;
    fn is_chain_ancestor_of(&self, low: DomainHash, high: DomainHash) -> Result<bool, ReachabilityError>;
    fn is_dag_ancestor_of(&self, low: DomainHash, high: DomainHash) -> Result<bool, ReachabilityError>;
    fn get_next_chain_ancestor(
        &self, descendant: DomainHash, ancestor: DomainHash,
    ) -> Result<DomainHash, ReachabilityError>;
}
