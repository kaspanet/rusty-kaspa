use std::sync::Arc;

use crate::header::Header;

pub type PruningPointProof = Vec<Vec<Arc<Header>>>;

pub type PruningPointsList = Vec<Arc<Header>>;
