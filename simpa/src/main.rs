use consensus::params::DEVNET_PARAMS;
use simulator::network::KaspaNetworkSimulator;

pub mod simulator;

fn main() {
    let bps = 8.0;
    let delay = 2.0;
    let num_miners = 8;
    let until = 1000 * 1000; // 1000 seconds
    let params = DEVNET_PARAMS.clone_with_skip_pow();
    let mut sim = KaspaNetworkSimulator::new(delay, bps, &params);
    sim.init(num_miners).run(until);
}
