#[cfg(not(feature = "heap"))]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
extern "C" {
    fn mi_option_set_enabled(_: mi_option_e, val: bool);
}

#[cfg(not(feature = "heap"))]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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

#[cfg(not(feature = "heap"))]
use mimalloc::MiMalloc;
#[cfg(not(feature = "heap"))]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub fn init_allocator_with_default_settings() {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    #[cfg(not(feature = "heap"))]
    unsafe {
        // Empirical tests show that this option results in the smallest RSS.
        mi_option_set_enabled(mi_option_e::mi_option_purge_decommits, false)
    };
}
