use std::sync::Arc;
use std::time::Duration;
use std::thread;
use std::thread::{spawn,JoinHandle};
use std::sync::atomic::{Ordering, AtomicBool};

use crate::trace;
use crate::core::Core;
use crate::service::Service;

pub struct TestService {
    terminate : AtomicBool,
    name : String,
}

impl TestService {

    pub fn new(
        name : &str,
    ) -> TestService {
        TestService {
            terminate : AtomicBool::new(false),
            name : name.to_string(),
        }
    }

    pub fn worker(self:&Arc<TestService>, _core : Arc<Core>) {
        loop {
            thread::sleep(Duration::from_millis(1000));
            trace!("{} ... {}",self.name, chrono::offset::Local::now());

            if self.terminate.load(Ordering::SeqCst) == true {
                break;
            }
        }

        trace!("{} thread exiting!", self.name);
    }

}

// service trait implementation for Monitor
impl Service for TestService {
    
    fn ident(self:Arc<TestService>) -> String {
        self.name.clone()
    }

    fn start(self:Arc<TestService>, core : Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self:Arc<TestService>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}
