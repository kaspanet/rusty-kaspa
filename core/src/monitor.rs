use std::sync::Arc;
use std::time::Duration;
use std::thread;
use std::thread::{spawn,JoinHandle};
use std::sync::atomic::{Ordering, AtomicBool};

use crate::trace;
use crate::core::Core;
use crate::service::Service;

pub struct Monitor {
    terminate : AtomicBool,
}

impl Monitor {

    pub fn new(

    ) -> Monitor {
        Monitor {
            terminate : AtomicBool::new(false),
        }
    }

    pub fn worker(self:&Arc<Monitor>, _core : Arc<Core>) {
        loop {
            thread::sleep(Duration::from_millis(1000));
            trace!("monitor ... {}",chrono::offset::Local::now());

            if self.terminate.load(Ordering::SeqCst) == true {
                break;
            }
        }

        trace!("monitor thread exiting!");
    }

}

// service trait implementation for Monitor
impl Service for Monitor {
    
    fn ident(self:Arc<Monitor>) -> String {
        "monitor".into()
    }

    fn start(self:Arc<Monitor>, core : Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self:Arc<Monitor>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}
