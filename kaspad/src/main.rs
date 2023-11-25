extern crate kaspa_consensus;
extern crate kaspa_core;
extern crate kaspa_hashes;

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use kaspa_core::{info, signals::Signals};
use kaspa_utils::fd_budget;
use kaspad_lib::{args::parse_args, daemon::create_core};

#[cfg(feature = "heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

pub fn main() {
    #[cfg(feature = "heap")]
    let _profiler = dhat::Profiler::builder().file_name("kaspad-heap.json").build();

    let args = parse_args();
    let fd_total_budget = fd_budget::limit() - args.rpc_max_clients as i32 - args.inbound_limit as i32 - args.outbound_target as i32;
    let (rx_bytes, tx_bytes): (Arc<AtomicUsize>, Arc<AtomicUsize>) = (Default::default(), Default::default());

    let (core, _) = create_core(args, fd_total_budget, rx_bytes, tx_bytes);

    // Bind the keyboard signal to the core
    Arc::new(Signals::new(&core)).init();

    core.run();
    info!("Kaspad has stopped...");
}
