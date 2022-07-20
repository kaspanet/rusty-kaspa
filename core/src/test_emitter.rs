use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

use crate::core::Core;
use crate::instruction::Instruction;
use crate::service::Service;
use crate::trace;

pub struct TestEmitter {
    terminate: AtomicBool,
    name: String,
    service: Sender<Instruction>,
    send_counter: Arc<AtomicU64>,
    sleep_time_msec: i64,
}

impl TestEmitter {
    pub fn new(
        name: &str, sleep_time_msec: i64, service: Sender<Instruction>, send_counter: Arc<AtomicU64>,
    ) -> TestEmitter {
        TestEmitter {
            terminate: AtomicBool::new(false),
            name: name.to_string(),
            service,
            send_counter,
            sleep_time_msec,
        }
    }

    pub fn worker(self: &Arc<TestEmitter>, _core: Arc<Core>) {
        let mut v: u64 = 0;

        loop {
            self.service
                .send(Instruction::TestInstructionForService(v))
                .unwrap();
            if self.terminate.load(Ordering::SeqCst) {
                break;
            }

            v += 1;
            self.send_counter.store(v, Ordering::SeqCst);

            if self.sleep_time_msec >= 0 {
                thread::sleep(Duration::from_millis(self.sleep_time_msec as u64));
            }
        }

        trace!("{} thread exiting", self.name);
    }
}

// service trait implementation for Monitor
impl Service for TestEmitter {
    fn ident(self: Arc<TestEmitter>) -> String {
        self.name.clone()
    }

    fn start(self: Arc<TestEmitter>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self: Arc<TestEmitter>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}
