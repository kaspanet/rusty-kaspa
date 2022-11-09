#![feature(generators, generator_trait)]

use rand_distr::{Distribution, Poisson};
use std::cell::RefCell;
use std::collections::{BinaryHeap, HashMap};
use std::ops::Generator;
use std::ops::GeneratorState::Yielded;
use std::pin::Pin;
use std::rc::Rc;

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

pub enum SimulationYield {
    Timeout(u64),
    Receive,
}

pub type Process = Box<dyn Unpin + Generator<Yield = SimulationYield, Return = ()>>;

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

    fn next(&mut self) -> u64 {
        let event = self.scheduler.pop().unwrap();
        self.time = event.timestamp;
        if let Some(msg) = event.msg {
            self.inboxes.insert(event.dest, msg);
        }
        event.dest
    }

    fn inbox(&mut self, dest: u64) -> Message {
        self.inboxes.remove(&dest).unwrap()
    }
}

#[derive(Default)]
struct Env {
    inner: RefCell<InnerEnv>,
}

impl Env {
    fn send(&self, delay: u64, dest: u64, msg: Message) {
        self.inner.borrow_mut().send(delay, dest, msg)
    }

    fn timeout(&self, timeout: u64, dest: u64) {
        self.inner.borrow_mut().timeout(timeout, dest)
    }

    pub fn next(&self) -> u64 {
        self.inner.borrow_mut().next()
    }

    pub fn run(&self, processes: &mut HashMap<u64, Process>, until: u64) {
        for i in processes.keys().copied().collect::<Vec<u64>>() {
            let Yielded(sim_yield) = Pin::new(processes.get_mut(&i).unwrap().as_mut()).resume(()) else { unreachable!() };
            match sim_yield {
                SimulationYield::Timeout(timeout) => self.timeout(timeout, i),
                SimulationYield::Receive => {}
            }
        }

        loop {
            let dest = self.next();
            let Yielded(sim_yield) = Pin::new(processes.get_mut(&dest).unwrap().as_mut()).resume(()) else { unreachable!() };
            match sim_yield {
                SimulationYield::Timeout(timeout) => self.timeout(timeout, dest),
                SimulationYield::Receive => {}
            }
            if self.inner.borrow().time > until {
                break;
            }
        }
    }

    pub fn inbox(&self, dest: u64) -> Message {
        self.inner.borrow_mut().inbox(dest)
    }
}

struct Sender {
    id: u64,
}

impl Sender {
    fn new(id: u64) -> Self {
        Self { id }
    }

    pub fn send(self, env: Rc<Env>) -> impl Generator<Yield = SimulationYield, Return = ()> {
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

    pub fn receive(self, env: Rc<Env>) -> impl Generator<Yield = SimulationYield, Return = ()> {
        move || loop {
            yield SimulationYield::Receive;
            println!("{}", env.inbox(self.id).msg);
        }
    }
}

fn main() {
    let env = Rc::new(Env::default());

    let mut processes = HashMap::<u64, Box<dyn Unpin + Generator<Yield = SimulationYield, Return = ()>>>::new();
    processes.insert(0, Box::new(Receiver::new(0).receive(env.clone())));
    processes.insert(1, Box::new(Sender::new(1).send(env.clone())));
    processes.insert(2, Box::new(Sender::new(2).send(env.clone())));

    env.run(&mut processes, 128);
}
