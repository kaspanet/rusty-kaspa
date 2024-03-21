#[cfg(feature = "heap")]
#[global_allocator]
#[cfg(not(feature = "heap"))]
static ALLOC: dhat::Alloc = dhat::Alloc;

pub mod common;
pub mod tasks;

#[cfg(test)]
pub mod consensus_integration_tests;

#[cfg(test)]
pub mod consensus_pipeline_tests;

#[cfg(test)]
pub mod daemon_integration_tests;

#[cfg(test)]
#[cfg(feature = "devnet-prealloc")]
pub mod mempool_benchmarks;

#[cfg(test)]
#[cfg(feature = "devnet-prealloc")]
pub mod subscribe_benchmarks;

#[cfg(test)]
pub mod rpc_tests;
