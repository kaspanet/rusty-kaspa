use thiserror::Error;

use crate::domain::consensus::model::{
    api::hash::DomainHash, staging_area::StagingArea, stores::errors::StoreError,
};

#[derive(Error, Debug)]
pub enum ReachabilityError {
    #[error("data store error")]
    ReachabilityStoreError(#[from] StoreError),
}

pub trait ReachabilityService {
    fn init(&mut self, staging: &dyn StagingArea) -> Result<(), ReachabilityError>;
    fn add(
        &mut self,
        staging: &dyn StagingArea,
        block: DomainHash,
        selected_parent: DomainHash,
        mergeset: &[DomainHash],
        is_selected_leaf: bool,
    ) -> Result<(), ReachabilityError>;
    fn is_chain_ancestor_of(
        &self,
        low: DomainHash,
        high: DomainHash,
    ) -> Result<bool, ReachabilityError>;
    fn is_dag_ancestor_of(
        &self,
        low: DomainHash,
        high: DomainHash,
    ) -> Result<bool, ReachabilityError>;
    fn get_next_chain_ancestor(
        &self,
        descendant: DomainHash,
        ancestor: DomainHash,
    ) -> Result<DomainHash, ReachabilityError>;
}
