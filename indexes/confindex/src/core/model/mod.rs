use kaspa_hashes::Hash;

pub type AcceptingBlueScore = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptingBlueScoreHashPair {
    pub accepting_blue_score: AcceptingBlueScore,
    pub hash: Hash,
}

impl AcceptingBlueScoreHashPair {
    pub fn new(accepting_blue_score: AcceptingBlueScore, hash: Hash) -> Self {
        Self { accepting_blue_score, hash }
    }
}

impl From<(AcceptingBlueScore, Hash)> for AcceptingBlueScoreHashPair {
    fn from((accepting_blue_score, hash): (AcceptingBlueScore, Hash)) -> Self {
        Self { accepting_blue_score, hash }
    }
}

pub struct AcceptingBlueScoreDiff {
    pub to_remove: Vec<AcceptingBlueScore>,
    pub to_add: Vec<AcceptingBlueScoreHashPair>,
}

impl AcceptingBlueScoreDiff {
    pub fn new(to_remove: Vec<AcceptingBlueScore>, to_add: Vec<AcceptingBlueScoreHashPair>) -> Self {
        Self { to_remove, to_add }
    }
}