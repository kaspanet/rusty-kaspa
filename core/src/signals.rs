use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

pub trait Shutdown {
    fn shutdown(self: &Arc<Self>);
}

pub struct Signals<T: 'static + Shutdown + Send + Sync> {
    target: Weak<T>,
    iterations: AtomicU64,
}

impl<T: Shutdown + Send + Sync> Signals<T> {
    pub fn new(target: &Arc<T>) -> Signals<T> {
        Signals { target: Arc::downgrade(target), iterations: AtomicU64::new(0) }
    }

    pub fn init(self: &Arc<Signals<T>>) {
        let core = self.target.clone();
        let signals = self.clone();
        ctrlc::set_handler(move || {
            let v = signals.iterations.fetch_add(1, Ordering::SeqCst);
            if v > 1 {
                println!("^SIGTERM - halting");
                std::process::exit(1);
            }

            println!("^SIGTERM - shutting down...");
            if let Some(actual_target) = core.upgrade() {
                actual_target.shutdown();
            }
        })
        .expect("Error setting signal handler");
    }
}
