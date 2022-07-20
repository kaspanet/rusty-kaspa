use crate::domain::consensus::model::services::reachability_service::ReachabilityService;

pub struct ReachabilityManager {}

impl ReachabilityManager {}

impl ReachabilityService for ReachabilityManager {
    fn init(
        &mut self,
        staging: &dyn crate::domain::consensus::model::staging_area::StagingArea,
    ) -> Result<
        (),
        crate::domain::consensus::model::services::reachability_service::ReachabilityError,
    > {
        todo!()
    }

    fn add(
        &mut self,
        staging: &dyn crate::domain::consensus::model::staging_area::StagingArea,
        block: crate::domain::consensus::model::api::hash::DomainHash,
        selected_parent: crate::domain::consensus::model::api::hash::DomainHash,
        mergeset: &[crate::domain::consensus::model::api::hash::DomainHash],
        is_selected_leaf: bool,
    ) -> Result<
        (),
        crate::domain::consensus::model::services::reachability_service::ReachabilityError,
    > {
        todo!()
    }

    fn is_chain_ancestor_of(
        &self,
        low: crate::domain::consensus::model::api::hash::DomainHash,
        high: crate::domain::consensus::model::api::hash::DomainHash,
    ) -> Result<
        bool,
        crate::domain::consensus::model::services::reachability_service::ReachabilityError,
    > {
        todo!()
    }

    fn is_dag_ancestor_of(
        &self,
        low: crate::domain::consensus::model::api::hash::DomainHash,
        high: crate::domain::consensus::model::api::hash::DomainHash,
    ) -> Result<
        bool,
        crate::domain::consensus::model::services::reachability_service::ReachabilityError,
    > {
        todo!()
    }

    fn get_next_chain_ancestor(
        &self,
        descendant: crate::domain::consensus::model::api::hash::DomainHash,
        ancestor: crate::domain::consensus::model::api::hash::DomainHash,
    ) -> Result<
        crate::domain::consensus::model::api::hash::DomainHash,
        crate::domain::consensus::model::services::reachability_service::ReachabilityError,
    > {
        todo!()
    }
}
