use super::errors::StoreError;
use crate::domain::consensus::{
    model::{api::hash::DomainHash, staging_area::StagingArea},
    processes::reachability::interval::Interval,
};

pub struct ReachabilityData {
    pub children: Vec<DomainHash>,
    pub parent: Option<DomainHash>,
    pub interval: Interval,
    pub future_covering_set: Vec<DomainHash>,
}

impl ReachabilityData {
    pub fn new(// children: Vec<DomainHash>,
        // parent: DomainHash,
        // interval: Interval,
        // future_covering_set: Vec<DomainHash>,
    ) -> Self {
        Self { children: vec![], parent: None, interval: Interval::maximal(), future_covering_set: vec![] }
    }
}

pub trait ReachabilityStore {
    fn stage(
        &mut self, staging: &dyn StagingArea, hash: &DomainHash, data: &ReachabilityData,
    ) -> Result<(), StoreError>;
    fn get(&self, staging: &dyn StagingArea, hash: &DomainHash) -> Result<ReachabilityData, StoreError>;
    fn has(&self, staging: &dyn StagingArea, hash: &DomainHash) -> Result<bool, StoreError>;
}
