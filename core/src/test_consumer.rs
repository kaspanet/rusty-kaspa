use crossbeam_channel::{select, unbounded, Receiver, Sender};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{spawn, JoinHandle};

use crate::core::Core;
use crate::instruction::Instruction;
use crate::service::Service;
use crate::trace;

pub struct TestConsumer {
    name: String,
    sender: Sender<Instruction>,
    receiver: Receiver<Instruction>,
    recv_count: Arc<AtomicU64>,
}

impl TestConsumer {
    pub fn new(name: &str, recv_count: Arc<AtomicU64>) -> TestConsumer {
        let (sender, receiver) = unbounded();
        TestConsumer { name: name.to_string(), sender, receiver, recv_count }
    }

    pub fn sender(&self) -> &Sender<Instruction> {
        &self.sender
    }

    pub fn worker(self: &Arc<TestConsumer>, _core: Arc<Core>) {
        let receiver = self.receiver.clone();
        loop {
            select! {
                recv(receiver) -> data => {
                    let op = data.unwrap();
                    match op {
                        Instruction::TestInstructionForConsumer(v) => {
                            // do something...
                            self.recv_count.store(v, Ordering::SeqCst);
                        },
                        Instruction::Shutdown => {
                            break;
                        },
                        // since we are using a single enum, we do not process consumer instructions
                        _ => { println!("consumer received invalid instruction: {:?}", op); }
                    }
                }
            }
        }

        trace!("{} thread exiting", self.name);
    }
}

// service trait implementation for Monitor
impl Service for TestConsumer {
    fn ident(self: Arc<TestConsumer>) -> String {
        self.name.clone()
    }

    fn start(self: Arc<TestConsumer>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self: Arc<TestConsumer>) {
        self.sender.send(Instruction::Shutdown).unwrap();
    }
}
