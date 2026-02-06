/// Error short codes for tracking worker errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorShortCode {
    NoMinerAddress,
    FailedBlockFetch,
    InvalidAddressFmt,
    MissingJob,
    BadDataFromMiner,
    FailedSendWork,
    FailedSetDiff,
    Disconnected,
}

impl ErrorShortCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorShortCode::NoMinerAddress => "err_no_miner_address",
            ErrorShortCode::FailedBlockFetch => "err_failed_block_fetch",
            ErrorShortCode::InvalidAddressFmt => "err_malformed_wallet_address",
            ErrorShortCode::MissingJob => "err_missing_job",
            ErrorShortCode::BadDataFromMiner => "err_bad_data_from_miner",
            ErrorShortCode::FailedSendWork => "err_failed_sending_work",
            ErrorShortCode::FailedSetDiff => "err_diff_set_failed",
            ErrorShortCode::Disconnected => "err_worker_disconnected",
        }
    }
}

impl std::fmt::Display for ErrorShortCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
