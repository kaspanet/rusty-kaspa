#![feature(generators, generator_trait)]

use rand_distr::{Distribution, Poisson};
use simulator::infra::{Env, Yield};
use std::collections::HashMap;
use std::ops::Generator;
use std::rc::Rc;

pub mod simulator;

#[derive(Default)]
struct Message {
    msg: String,
}

impl Message {
    fn new(msg: String) -> Self {
        Self { msg }
    }
}

struct Sender {
    id: u64,
}

impl Sender {
    fn new(id: u64) -> Self {
        Self { id }
    }

    pub fn send(self, env: Rc<Env<Message>>) -> impl Generator<Yield = Yield, Return = ()> {
        move || {
            let poi = Poisson::new(8f64).unwrap();
            let mut thread_rng = rand::thread_rng();
            let mut i = 0;
            loop {
                i += 1;
                let msg = Message::new(format!("Message #{} from {}", i, self.id));
                let delay = 0;
                env.send(delay, 0, msg);

                yield Yield::Timeout(poi.sample(&mut thread_rng) as u64);
            }
        }
    }
}

struct Receiver {
    id: u64,
}

impl Receiver {
    fn new(id: u64) -> Self {
        Self { id }
    }

    pub fn receive(self, env: Rc<Env<Message>>) -> impl Generator<Yield = Yield, Return = ()> {
        move || loop {
            yield Yield::Wait;
            println!("{}", env.inbox(self.id).msg);
        }
    }
}

fn main() {
    let env = Rc::new(Env::<Message>::default());

    let mut processes = HashMap::<u64, Box<dyn Unpin + Generator<Yield = Yield, Return = ()>>>::new();
    processes.insert(0, Box::new(Receiver::new(0).receive(env.clone())));
    processes.insert(1, Box::new(Sender::new(1).send(env.clone())));
    processes.insert(2, Box::new(Sender::new(2).send(env.clone())));

    env.run(&mut processes, 128);
}
