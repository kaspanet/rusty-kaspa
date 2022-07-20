use kaspa_core::core::Core;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct Signals {
    core: Arc<Core>,
    iterations: AtomicU64,
}

impl Signals {
    pub fn new(core: Arc<Core>) -> Signals {
        Signals { core, iterations: AtomicU64::new(0) }
    }

    pub fn init(self: &Arc<Signals>) {
        let core = self.core.clone();
        let signals = self.clone();
        ctrlc::set_handler(move || {
            let v = signals.iterations.fetch_add(1, Ordering::SeqCst);
            if v > 1 {
                println!("^SIGNAL - halting");
                std::process::exit(1);
            }

            println!("^SIGNAL - shutting down core... (CTRL+C again to halt)");
            core.shutdown();
        })
        .expect("Error setting signal handler");
    }
}
