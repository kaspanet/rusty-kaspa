use super::infra::Yield as SimYield;
use super::network::KaspaNetworkSimulator;
use consensus_core::block::Block;
use rand_distr::{Distribution, Exp};
use std::cmp::max;
use std::ops::Generator;
use std::rc::Rc;

pub struct Miner {
    // IDs
    pub(super) mine_id: u64,
    pub(super) recv_id: u64,

    // Config
    lambda: f64,   // In millisecond units
    hashrate: f64, // Relative hashrate of this miner
}

impl Miner {
    pub fn new(mine_id: u64, recv_id: u64, bps: f64, hashrate: f64) -> Self {
        Self { mine_id, recv_id, lambda: bps / 1000.0, hashrate }
    }

    fn new_block(&self) -> Block {
        unimplemented!()
    }

    pub fn mine(self: Rc<Self>, sim: Rc<KaspaNetworkSimulator>) -> impl Generator<Yield = SimYield, Return = ()> {
        move || {
            let dist = Exp::new(1f64 / self.lambda * self.hashrate).unwrap();
            let mut thread_rng = rand::thread_rng();
            loop {
                yield SimYield::Timeout(max(dist.sample(&mut thread_rng) as u64, 1));
                let block = self.new_block();
                sim.broadcast(self.mine_id, block);
            }
        }
    }

    pub fn receive(self: Rc<Self>, sim: Rc<KaspaNetworkSimulator>) -> impl Generator<Yield = SimYield, Return = ()> {
        move || loop {
            yield SimYield::Wait;
            println!("{:?}", sim.env.inbox(self.recv_id));
        }
    }
}
