#[ctor::ctor]
fn init_allocator() {
    kaspa_alloc::init_allocator_with_default_settings();
}

pub mod common;

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
pub mod rpc_tests;
