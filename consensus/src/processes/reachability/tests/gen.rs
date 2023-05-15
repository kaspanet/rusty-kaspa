use kaspa_utils::sim::{Environment, Process, Resumption, Simulation, Suspension};
use rand::rngs::ThreadRng;
use rand_distr::{Distribution, Exp};
use std::{cell::RefCell, cmp::max, collections::HashSet, iter::once, rc::Rc};

type Message = (u64, Vec<u64>);

#[derive(Default, Clone)]
struct Dag {
    genesis: u64,
    blocks: Rc<RefCell<Vec<Message>>>,
}

impl Dag {
    fn new(genesis: u64) -> Self {
        Self { genesis, blocks: Default::default() }
    }
}

struct Miner {
    // ID
    pub(super) id: u64,

    // Rand
    dist: Exp<f64>, // The time interval between Poisson(lambda) events distributes ~Exp(lambda)
    rng: ThreadRng,

    // Counters
    next_block: u64,
    num_blocks: u64,

    // Config
    target_blocks: Option<u64>,

    // Accumulated data
    dag: Dag,
    tips: HashSet<u64>,
}

impl Miner {
    pub fn new(id: u64, bps: f64, hashrate: f64, target_blocks: Option<u64>, dag: Dag) -> Self {
        let genesis = dag.genesis;
        Self {
            id,
            dist: Exp::new(bps * hashrate).unwrap(),
            rng: rand::thread_rng(),
            next_block: genesis + 1,
            num_blocks: 0,
            target_blocks,
            dag,
            tips: HashSet::from_iter(once(genesis)),
        }
    }

    pub fn new_unique(&mut self) -> u64 {
        let c = self.next_block;
        self.next_block += 1;
        c
    }

    fn build_new_block(&mut self, _timestamp: u64) -> Message {
        (self.new_unique(), self.tips.iter().copied().collect())
    }

    pub fn mine(&mut self, env: &mut Environment<Message>) -> Suspension {
        let block = self.build_new_block(env.now());
        env.broadcast(self.id, block);
        self.sample_mining_interval()
    }

    fn sample_mining_interval(&mut self) -> Suspension {
        Suspension::Timeout(max((self.dist.sample(&mut self.rng) * 1000.0) as u64, 1))
    }

    fn process_message(&mut self, msg: Message, env: &mut Environment<Message>) -> Suspension {
        if self.check_halt(env) {
            Suspension::Halt
        } else {
            // Process the msg
            for p in msg.1.iter() {
                self.tips.remove(p);
            }
            self.tips.insert(msg.0);
            self.dag.blocks.borrow_mut().push(msg);
            Suspension::Idle
        }
    }

    fn check_halt(&mut self, _env: &mut Environment<Message>) -> bool {
        self.num_blocks += 1;
        if let Some(target_blocks) = self.target_blocks {
            if self.num_blocks > target_blocks {
                return true; // Exit
            }
        }
        false
    }
}

impl Process<Message> for Miner {
    fn resume(&mut self, resumption: Resumption<Message>, env: &mut Environment<Message>) -> Suspension {
        match resumption {
            Resumption::Initial => self.sample_mining_interval(),
            Resumption::Scheduled => self.mine(env),
            Resumption::Message(msg) => self.process_message(msg, env),
        }
    }
}

pub fn generate_complex_dag(delay: f64, bps: f64, target_blocks: u64) -> (u64, Vec<Message>) {
    let genesis = 1;
    let dag = Dag::new(genesis);
    let mut simulation = Simulation::new((delay * 1000.0) as u64);
    let miner_process = Box::new(Miner::new(0, bps, 1f64, Some(target_blocks), dag.clone()));
    simulation.register(0, miner_process);
    simulation.run(u64::MAX);
    (dag.genesis, dag.blocks.take())
}
