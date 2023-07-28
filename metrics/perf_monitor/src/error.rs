use perf_monitor::io::IOStatsError;
use std::io;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error(transparent)]
    IOStats(#[from] IOStatsError),
}
