use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::header::Header;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaaScoreTimestamp {
    pub daa_score: u64,
    pub timestamp: u64,
}

impl From<Header> for DaaScoreTimestamp {
    fn from(header: Header) -> DaaScoreTimestamp {
        DaaScoreTimestamp { daa_score: header.daa_score, timestamp: header.timestamp }
    }
}

impl From<Arc<Header>> for DaaScoreTimestamp {
    fn from(header: Arc<Header>) -> DaaScoreTimestamp {
        DaaScoreTimestamp { daa_score: header.daa_score, timestamp: header.timestamp }
    }
}
