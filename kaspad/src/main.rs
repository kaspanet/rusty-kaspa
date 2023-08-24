extern crate kaspa_consensus;
extern crate kaspa_core;
extern crate kaspa_hashes;

use std::sync::Arc;

use kaspa_core::{signals::Signals, trace};
use kaspad::{args::parse_args, daemon::create_core};

// TODO: refactor the shutdown sequence into a predefined controlled sequence

pub fn main() {
    #[cfg(feature = "heap")]
    let _profiler = dhat::Profiler::builder().file_name("kaspad-heap.json").build();

    let args = parse_args();
    let core = create_core(args);

    // Bind the keyboard signal to the core
    Arc::new(Signals::new(&core)).init();

    core.run();
    trace!("Kaspad is finished...");
}
