use std::sync::Arc;

use crate::stores::accepting_blue_score::DbScoreIndexAcceptingBlueScoreStore;
struct StoreManager {
    pub accepting_blue_score_store: Arc<DbScoreIndexAcceptingBlueScoreStore>,
}

impl StoreManager {
    pub fn new(accepting_blue_score_store: Arc<DbScoreIndexAcceptingBlueScoreStore>) -> Self {
        Self { accepting_blue_score_store }
    }
}
