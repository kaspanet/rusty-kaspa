use std::sync::Arc;
use std::thread::{spawn,JoinHandle};
use crossbeam_channel::{unbounded, Sender, Receiver, select};

use crate::trace;
use crate::core::Core;
use crate::service::Service;
use crate::instruction::Instruction;

pub struct TestService {
    name : String,
    threads : usize,
    sender : Sender<Instruction>,
    receiver : Receiver<Instruction>,
    consumer : Sender<Instruction>,
}

impl TestService {

    pub fn new(
        name : &str,
        threads : usize,
        consumer : Sender<Instruction>,
    ) -> TestService {
        let (sender, receiver) = unbounded();
        TestService {
            name : name.to_string(),
            threads,
            sender,
            receiver,
            consumer,
        }
    }

    pub fn sender(&self) -> &Sender<Instruction> {
        &self.sender
    }

    pub fn worker(self:&Arc<TestService>, _core : Arc<Core>, id : usize) {
        loop {

            let receiver = self.receiver.clone();

            select! {

                recv(receiver) -> data => {
                    let op = data.unwrap();
                    match op {
                        Instruction::TestInstructionForService(v) => {
                            // do something... then relay to consumer...
                            self.consumer.send(Instruction::TestInstructionForConsumer(v)).unwrap();
                        },
                        Instruction::Shutdown => {
                            break;
                        },
                        // since we are using a single enum, we do not process consumer instructions
                        _ => { println!("service received invalid instruction: {:?}", op); }
                    }
                }
            }
        }

        trace!("{}[{}] ... thread exiting", self.name, id);
    }

}

// service trait implementation for Monitor
impl Service for TestService {
    
    fn ident(self:Arc<TestService>) -> String {
        self.name.clone()
    }

    fn start(self:Arc<TestService>, core : Arc<Core>) -> Vec<JoinHandle<()>> {
        let mut workers = Vec::new();
        for id in 0..self.threads {
            let service = self.clone();
            let core = core.clone();
            workers.push(spawn(move || service.worker(core,id)));
        }
        workers
    }

    fn stop(self:Arc<TestService>) {
        for _ in 0..self.threads {
            self.sender.send(Instruction::Shutdown).unwrap();
        }
    }
}

