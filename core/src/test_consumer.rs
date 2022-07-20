use std::sync::Arc;
use std::thread::{spawn,JoinHandle};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use crossbeam_channel::{unbounded, Sender, Receiver, select};

use crate::trace;
use crate::core::Core;
use crate::service::Service;
use crate::instruction::Instruction;

pub struct TestConsumer {
    terminate : AtomicBool,
    name : String,
    sender : Sender<Instruction>,
    receiver : Receiver<Instruction>,
    recv_count : Arc<AtomicU64>,
}

impl TestConsumer {

    pub fn new(
        name : &str,
        recv_count : Arc<AtomicU64>,
    ) -> TestConsumer {
        let (sender, receiver) = unbounded();
        TestConsumer {
            terminate : AtomicBool::new(false),
            name : name.to_string(),
            sender,
            receiver,
            recv_count,
        }
    }

    pub fn sender(&self) -> &Sender<Instruction> {
        &self.sender
    }

    pub fn worker(self:&Arc<TestConsumer>, _core : Arc<Core>) {
        loop {

            let receiver = self.receiver.clone();

            select! {

                recv(receiver) -> data => {
                    let op = data.unwrap();
                    match op {
                        Instruction::TestInstructionForConsumer(v) => {
                            // do something...
                            self.recv_count.store(v,Ordering::SeqCst);
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
    
    fn ident(self:Arc<TestConsumer>) -> String {
        self.name.clone()
    }

    fn start(self:Arc<TestConsumer>, core : Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self:Arc<TestConsumer>) {
        self.sender.send(Instruction::Shutdown).unwrap();
    }
}
