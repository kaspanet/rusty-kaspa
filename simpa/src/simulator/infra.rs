use std::ops::GeneratorState::{Complete, Yielded};
use std::pin::Pin;
use std::{
    cell::RefCell,
    collections::{BinaryHeap, HashMap},
    ops::Generator,
};

pub struct Event<T> {
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

pub enum Yield {
    Timeout(u64),
    Wait,
}

pub type Process = Box<dyn Unpin + Generator<Yield = Yield, Return = ()>>;

#[derive(Default)]
struct InnerEnv<T> {
    time: u64,
    scheduler: BinaryHeap<Event<T>>,
    inboxes: HashMap<u64, T>,
}

impl<T> InnerEnv<T> {
    fn new() -> Self {
        Self { time: 0, scheduler: BinaryHeap::new(), inboxes: HashMap::new() }
    }

    fn send(&mut self, delay: u64, dest: u64, msg: T) {
        self.scheduler.push(Event::new(self.time + delay, dest, Some(msg)))
    }

    fn timeout(&mut self, timeout: u64, dest: u64) {
        self.scheduler.push(Event::new(self.time + timeout, dest, None))
    }

    fn next(&mut self) -> u64 {
        let event = self.scheduler.pop().unwrap();
        self.time = event.timestamp;
        if let Some(msg) = event.msg {
            self.inboxes.insert(event.dest, msg);
        }
        event.dest
    }

    fn inbox(&mut self, dest: u64) -> T {
        self.inboxes.remove(&dest).unwrap()
    }
}

#[derive(Default)]
pub struct Env<T> {
    inner: RefCell<InnerEnv<T>>,
}

impl<T> Env<T> {
    pub fn new() -> Self {
        Self { inner: RefCell::new(InnerEnv::new()) }
    }

    pub fn send(&self, delay: u64, dest: u64, msg: T) {
        self.inner.borrow_mut().send(delay, dest, msg)
    }

    pub fn timeout(&self, timeout: u64, dest: u64) {
        self.inner.borrow_mut().timeout(timeout, dest)
    }

    pub fn next(&self) -> u64 {
        self.inner.borrow_mut().next()
    }

    pub fn step(&self, id: u64, process: &mut Process) {
        match Pin::new(process).resume(()) {
            Yielded(yielded) => match yielded {
                Yield::Timeout(timeout) => self.timeout(timeout, id),
                Yield::Wait => {}
            },
            Complete(_) => {}
        }
    }

    pub fn run(&self, processes: &mut HashMap<u64, Process>, until: u64) {
        for i in processes.keys().copied().collect::<Vec<u64>>() {
            self.step(i, processes.get_mut(&i).unwrap());
        }

        loop {
            let dest = self.next();
            self.step(dest, processes.get_mut(&dest).unwrap());
            if self.inner.borrow().time > until {
                break;
            }
        }
    }

    pub fn inbox(&self, dest: u64) -> T {
        self.inner.borrow_mut().inbox(dest)
    }
}
