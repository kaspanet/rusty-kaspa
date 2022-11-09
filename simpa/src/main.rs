#![feature(generators, generator_trait)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ops::Generator;
use std::pin::Pin;

#[derive(Default)]
struct State {
    buffer: RefCell<VecDeque<i32>>,
}

impl State {
    pub fn push(&self, val: i32) {
        self.buffer.borrow_mut().push_back(val);
    }

    pub fn pop(&self) -> i32 {
        self.buffer.borrow_mut().pop_front().unwrap()
    }
}

struct Sender {}

impl Sender {
    fn new() -> Self {
        Self {}
    }

    pub fn send(self, state: &State) -> impl Generator<Yield = u64, Return = ()> + '_ {
        || {
            let mut i = 0;
            loop {
                i -= 1;
                state.push(i);
                yield 1;
            }
        }
    }
}

struct Receiver {}

impl Receiver {
    fn new() -> Self {
        Self {}
    }

    pub fn receive(self, state: &State) -> impl Generator<Yield = (), Return = ()> + '_ {
        || loop {
            println!("{}", state.pop());
            yield;
        }
    }
}

fn main() {
    let state = State::default();
    let mut sender = Sender::new().send(&state);
    let mut receiver = Receiver::new().receive(&state);

    for _ in 0..10 {
        Pin::new(&mut sender).resume(());
        Pin::new(&mut receiver).resume(());
    }
}
