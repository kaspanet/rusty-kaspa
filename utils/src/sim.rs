//! Module with structs for supporting discrete event simulation in virtual time.
//! Inspired by python's simpy library.
//!
//! Users should define the message type `T` required for the simulation, derive `Process<T>` with
//! various simulation actor logic and plug the processes into a `Simulation<T>` instance.

use std::collections::{BinaryHeap, HashMap, HashSet};

/// Internal structure representing a scheduled simulator event
struct Event<T> {
    timestamp: u64,
    dest: u64,
    msg: Option<T>,
}

impl<T> Event<T> {
    pub fn new(timestamp: u64, dest: u64, msg: Option<T>) -> Self {
        Self { timestamp, dest, msg }
    }
}

impl<T> PartialEq for Event<T> {
    fn eq(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp && self.dest == other.dest
    }
}

impl<T> Eq for Event<T> {}

impl<T> PartialOrd for Event<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Event<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reversing so that min timestamp is scheduled first
        other.timestamp.cmp(&self.timestamp).then_with(|| other.dest.cmp(&self.dest))
    }
}

/// Process resumption trigger
pub enum Resumption<T> {
    Initial,
    Scheduled,
    Message(T),
}

/// Process suspension reason
pub enum Suspension {
    Timeout(u64),
    Idle,
    Halt, // Halt the simulation
}

/// A simulation process
pub trait Process<T> {
    fn resume(&mut self, resumption: Resumption<T>, env: &mut Environment<T>) -> Suspension;
}

pub type BoxedProcess<T> = Box<dyn Process<T>>;

/// The simulation environment
#[derive(Default)]
pub struct Environment<T> {
    now: u64,
    broadcast_delay: u64,
    event_queue: BinaryHeap<Event<T>>,
    process_ids: HashSet<u64>,
}

impl<T: Clone> Environment<T> {
    pub fn new(delay: u64) -> Self {
        Self::with_start_time(delay, 0)
    }

    pub fn with_start_time(delay: u64, start_time: u64) -> Self {
        Self { now: start_time, broadcast_delay: delay, event_queue: BinaryHeap::new(), process_ids: HashSet::new() }
    }

    pub fn now(&self) -> u64 {
        self.now
    }

    pub fn send(&mut self, delay: u64, dest: u64, msg: T) {
        self.event_queue.push(Event::new(self.now + delay, dest, Some(msg)))
    }

    pub fn timeout(&mut self, timeout: u64, dest: u64) {
        self.event_queue.push(Event::new(self.now + timeout, dest, None))
    }

    pub fn broadcast(&mut self, _sender: u64, msg: T) {
        for &id in self.process_ids.iter() {
            self.event_queue.push(Event::new(self.now + self.broadcast_delay, id, Some(msg.clone())));
        }
    }

    fn next_event(&mut self) -> Event<T> {
        let event = self.event_queue.pop().unwrap();
        self.now = event.timestamp;
        event
    }
}

/// The simulation manager
#[derive(Default)]
pub struct Simulation<T> {
    env: Environment<T>,
    processes: HashMap<u64, BoxedProcess<T>>,
}

impl<T: Clone> Simulation<T> {
    pub fn new(delay: u64) -> Self {
        Self { env: Environment::new(delay), processes: HashMap::new() }
    }

    pub fn with_start_time(delay: u64, start_time: u64) -> Self {
        Self { env: Environment::with_start_time(delay, start_time), processes: HashMap::new() }
    }

    pub fn register(&mut self, id: u64, process: BoxedProcess<T>) {
        self.processes.insert(id, process);
        self.env.process_ids.insert(id);
    }

    pub fn step(&mut self) -> bool {
        let event = self.env.next_event();
        let process = self.processes.get_mut(&event.dest).unwrap();
        let op = if let Some(msg) = event.msg { Resumption::Message(msg) } else { Resumption::Scheduled };
        match process.resume(op, &mut self.env) {
            Suspension::Timeout(timeout) => {
                self.env.timeout(timeout, event.dest);
                true
            }
            Suspension::Idle => true,
            Suspension::Halt => false,
        }
    }

    pub fn run(&mut self, until: u64) {
        for (&id, process) in self.processes.iter_mut() {
            match process.resume(Resumption::Initial, &mut self.env) {
                Suspension::Timeout(timeout) => self.env.timeout(timeout, id),
                Suspension::Idle => {}
                Suspension::Halt => panic!("not expecting halt on startup"),
            }
        }

        while self.step() {
            if self.env.now() > until {
                break;
            }
        }
        self.processes.clear();
    }
}
