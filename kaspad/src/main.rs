extern crate kaspa_consensus;
extern crate kaspa_core;
extern crate kaspa_hashes;

use std::sync::Arc;

use kaspa_core::{info, signals::Signals};
use kaspa_utils::fd_budget;
use kaspad_lib::{
    args::parse_args,
    daemon::{create_core, DESIRED_DAEMON_SOFT_FD_LIMIT, MINIMUM_DAEMON_SOFT_FD_LIMIT},
};

#[cfg(not(feature = "heap"))]
#[cfg(unix)]
extern "C" {
    fn mi_option_set_enabled(_: mi_option_e, val: bool);
}

#[cfg(not(feature = "heap"))]
#[cfg(unix)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
#[repr(C)]
enum mi_option_e {
    // stable options
    mi_option_show_errors, // print error messages
    mi_option_show_stats,  // print statistics on termination
    mi_option_verbose,     // print verbose messages
    // the following options are experimental (see src/options.h)
    mi_option_eager_commit,             // eager commit segments? (after `eager_commit_delay` segments) (=1)
    mi_option_arena_eager_commit,       // eager commit arenas? Use 2 to enable just on overcommit systems (=2)
    mi_option_purge_decommits,          // should a memory purge decommit (or only reset) (=1)
    mi_option_allow_large_os_pages,     // allow large (2MiB) OS pages, implies eager commit
    mi_option_reserve_huge_os_pages,    // reserve N huge OS pages (1GiB/page) at startup
    mi_option_reserve_huge_os_pages_at, // reserve huge OS pages at a specific NUMA node
    mi_option_reserve_os_memory,        // reserve specified amount of OS memory in an arena at startup
    mi_option_deprecated_segment_cache,
    mi_option_deprecated_page_reset,
    mi_option_abandoned_page_purge, // immediately purge delayed purges on thread termination
    mi_option_deprecated_segment_reset,
    mi_option_eager_commit_delay,
    mi_option_purge_delay, // memory purging is delayed by N milli seconds; use 0 for immediate purging or -1 for no purging at all.
    mi_option_use_numa_nodes, // 0 = use all available numa nodes, otherwise use at most N nodes.
    mi_option_limit_os_alloc, // 1 = do not use OS memory for allocation (but only programmatically reserved arenas)
    mi_option_os_tag,      // tag used for OS logging (macOS only for now)
    mi_option_max_errors,  // issue at most N error messages
    mi_option_max_warnings, // issue at most N warning messages
    mi_option_max_segment_reclaim,
    mi_option_destroy_on_exit, // if set, release all memory on exit; sometimes used for dynamic unloading but can be unsafe.
    mi_option_arena_reserve,   // initial memory size in KiB for arena reservation (1GiB on 64-bit)
    mi_option_arena_purge_mult,
    mi_option_purge_extend_delay,
    _mi_option_last,
}

#[cfg(feature = "heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[cfg(not(feature = "heap"))]
use mimalloc::MiMalloc;
#[cfg(not(feature = "heap"))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub fn main() {
    #[cfg(feature = "heap")]
    let _profiler = dhat::Profiler::builder().file_name("kaspad-heap.json").build();

    #[cfg(unix)]
    #[cfg(not(feature = "heap"))]
    unsafe {
        mi_option_set_enabled(mi_option_e::mi_option_purge_decommits, false)
    };

    let args = parse_args();

    match fd_budget::try_set_fd_limit(DESIRED_DAEMON_SOFT_FD_LIMIT) {
        Ok(limit) => {
            if limit < MINIMUM_DAEMON_SOFT_FD_LIMIT {
                println!("Current OS file descriptor limit (soft FD limit) is set to {limit}");
                println!("The kaspad node requires a setting of at least {DESIRED_DAEMON_SOFT_FD_LIMIT} to operate properly.");
                println!("Please increase the limits using the following command:");
                println!("ulimit -n {DESIRED_DAEMON_SOFT_FD_LIMIT}");
            }
        }
        Err(err) => {
            println!("Unable to initialize the necessary OS file descriptor limit (soft FD limit) to: {}", err);
            println!("The kaspad node requires a setting of at least {DESIRED_DAEMON_SOFT_FD_LIMIT} to operate properly.");
        }
    }

    let fd_total_budget = fd_budget::limit() - args.rpc_max_clients as i32 - args.inbound_limit as i32 - args.outbound_target as i32;
    let (core, _) = create_core(args, fd_total_budget);

    // Bind the keyboard signal to the core
    Arc::new(Signals::new(&core)).init();

    core.run();
    info!("Kaspad has stopped...");
}
