#![feature(generators, generator_trait)]

use simulator::network::KaspaNetworkSimulator;
use std::rc::Rc;

pub mod simulator;

fn main() {
    let bps = 8.0;
    let delay = 2.0;
    let num_miners = 8;
    let until = 1000 * 1000; // 1000 seconds
    Rc::new(KaspaNetworkSimulator::new(delay, bps)).init(num_miners).run(until);
}
