use super::infra::{Env, Process};
use super::miner::Miner;
use consensus::params::Params;
use consensus_core::block::Block;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct KaspaNetworkSimulator {
    // Internal simulation env
    pub(super) env: Env<Block>,

    // Simulation processes
    miners: RefCell<Vec<Rc<Miner>>>,
    processes: RefCell<HashMap<u64, Process>>,

    // Config
    params: Params,
    delay: f64, // In millisecond units
    bps: f64,   // Blocks per second
}

impl KaspaNetworkSimulator {
    pub fn new(delay: f64, bps: f64, params: &Params) -> Self {
        Self {
            env: Env::new(),
            miners: RefCell::new(Vec::new()),
            processes: RefCell::new(HashMap::new()),
            delay,
            bps,
            params: params.clone(),
        }
    }

    pub fn init(self: Rc<Self>, num_miners: u64) -> Rc<Self> {
        for i in 0..num_miners {
            let (mine_id, recv_id) = (i * 2, i * 2 + 1);
            let miner = Rc::new(Miner::new(mine_id, recv_id, self.bps, 1f64 / num_miners as f64, &self.params));
            self.processes.borrow_mut().insert(mine_id, Box::new(miner.clone().mine(self.clone())));
            self.processes.borrow_mut().insert(recv_id, Box::new(miner.clone().receive(self.clone())));
            self.miners.borrow_mut().push(miner);
        }
        self
    }

    pub fn broadcast(self: &Rc<Self>, sender: u64, block: Block) {
        for miner in self.miners.borrow().iter() {
            let delay = if miner.mine_id == sender || miner.recv_id == sender { 0 } else { self.delay as u64 };
            self.env.send(delay, miner.recv_id, block.clone());
        }
    }

    pub fn run(self: &Rc<Self>, until: u64) {
        self.env.run(&mut self.processes.borrow_mut(), until);
        for miner in self.miners.borrow_mut().drain(..) {
            miner.consensus.shutdown(miner.handles.borrow_mut().take().unwrap());
        }
    }
}
