#![feature(generators, generator_trait)]

use rand_distr::{Distribution, Poisson};
use std::cell::RefCell;
use std::collections::{BinaryHeap, HashMap};
use std::ops::Generator;
use std::ops::GeneratorState::Yielded;
use std::pin::Pin;

struct Message {
    msg: String,
}

impl Message {
    fn new(msg: String) -> Self {
        Self { msg }
    }
}

struct SimulationEvent {
    timestamp: u64,
    dest: u64,
    msg: Option<Message>,
}

impl SimulationEvent {
    fn new(timestamp: u64, dest: u64, msg: Option<Message>) -> Self {
        Self { timestamp, dest, msg }
    }
}

impl PartialEq for SimulationEvent {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp && self.dest == other.dest
    }
}

impl Eq for SimulationEvent {}

impl PartialOrd for SimulationEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SimulationEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reversing so that min timestamp is scheduled first
        other.timestamp.cmp(&self.timestamp).then_with(|| other.dest.cmp(&self.dest))
    }
}

enum SimulationYield {
    Timeout(u64),
    Receive,
}

#[derive(Default)]
struct InnerEnv {
    time: u64,
    scheduler: BinaryHeap<SimulationEvent>,
    inboxes: HashMap<u64, Message>,
}

impl InnerEnv {
    fn send(&mut self, delay: u64, dest: u64, msg: Message) {
        self.scheduler.push(SimulationEvent::new(self.time + delay, dest, Some(msg)))
    }

    fn timeout(&mut self, timeout: u64, dest: u64) {
        self.scheduler.push(SimulationEvent::new(self.time + timeout, dest, None))
    }
}

#[derive(Default)]
struct Env {
    inner: RefCell<InnerEnv>,
}

impl Env {
    // pub fn schedule(&self, event: SimulationEvent) {
    //     self.inner.borrow_mut().scheduler.push(event)
    // }

    fn send(&self, delay: u64, dest: u64, msg: Message) {
        self.inner.borrow_mut().send(delay, dest, msg)
    }

    fn timeout(&self, timeout: u64, dest: u64) {
        self.inner.borrow_mut().timeout(timeout, dest)
    }

    pub fn next(&self) -> u64 {
        let event = self.inner.borrow_mut().scheduler.pop().unwrap();
        self.inner.borrow_mut().time = event.timestamp;
        if let Some(msg) = event.msg {
            self.inner.borrow_mut().inboxes.insert(event.dest, msg);
        }
        event.dest
    }

    pub fn inbox(&self, dest: u64) -> Message {
        self.inner.borrow_mut().inboxes.remove(&dest).unwrap()
    }
}

struct Sender {
    id: u64,
}

impl Sender {
    fn new(id: u64) -> Self {
        Self { id }
    }

    pub fn send(self, env: &Env) -> impl Generator<Yield = SimulationYield, Return = ()> + '_ {
        move || {
            let poi = Poisson::new(8f64).unwrap();
            let mut thread_rng = rand::thread_rng();
            let mut i = 0;
            loop {
                i += 1;
                let msg = Message::new(format!("Message #{} from {}", i, self.id));
                let delay = 0;
                env.send(delay, 0, msg);

                let timeout = poi.sample(&mut thread_rng) as u64;
                yield SimulationYield::Timeout(timeout);
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

    pub fn receive(self, env: &Env) -> impl Generator<Yield = SimulationYield, Return = ()> + '_ {
        move || loop {
            yield SimulationYield::Receive; // TODO
            println!("{}", env.inbox(self.id).msg);
        }
    }
}

fn main() {
    let env = Env::default();

    let mut processes = HashMap::<u64, Box<dyn Unpin + Generator<Yield = SimulationYield, Return = ()>>>::new();
    processes.insert(0, Box::new(Receiver::new(0).receive(&env)));
    processes.insert(1, Box::new(Sender::new(1).send(&env)));
    processes.insert(2, Box::new(Sender::new(2).send(&env)));

    for i in 0u64..=2 {
        let Yielded(sim_yield) = Pin::new(processes.get_mut(&i).unwrap().as_mut()).resume(()) else { unreachable!() };
        match sim_yield {
            SimulationYield::Timeout(timeout) => env.timeout(timeout, i),
            SimulationYield::Receive => {}
        }
    }

    for _ in 0..32 {
        let dest = env.next();
        let Yielded(sim_yield) = Pin::new(processes.get_mut(&dest).unwrap().as_mut()).resume(()) else { unreachable!() };
        match sim_yield {
            SimulationYield::Timeout(timeout) => env.timeout(timeout, dest),
            SimulationYield::Receive => {}
        }
    }
}
