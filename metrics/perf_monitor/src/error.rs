use std::io;
use workflow_perf_monitor::io::IOStatsError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error(transparent)]
    IOStats(#[from] IOStatsError),
}
