use kaspa_core::core::Core;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64,Ordering};

pub struct Signals {
    core : Arc<Core>,
    iterations : AtomicU64,
}

impl Signals {
    pub fn new(core: Arc<Core>) -> Signals {
        Signals {
            core,
            iterations : AtomicU64::new(0),
        }
    }

    pub fn init(self:&Arc<Signals>) {

        let core = self.core.clone();
        let signals = self.clone();
        ctrlc::set_handler(move || {

            let v = signals.iterations.load(Ordering::SeqCst);
            if v > 1 {
                println!("^SIGNAL - halting");
                std::process::exit(1);
            }
            signals.iterations.store(v+1, Ordering::SeqCst);

            println!("^SIGNAL - shutting down core... (CTRL+C again to halt)");
            core.shutdown();
        }).expect("Error setting signal handler");
    }
}
