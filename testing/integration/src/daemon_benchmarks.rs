use crate::common::daemon::Daemon;
use kaspa_addresses::Address;
use kaspa_consensus::params::Params;
use kaspa_core::{debug, signals::Shutdown};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspad::args::Args;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use std::{cmp::max, time::Duration};

#[tokio::test]
async fn bench_bbt_latency() {
    kaspa_core::log::try_init_logger("info");
    //
    // Setup
    //
    let args = Args { simnet: true, enable_unsynced_mining: true, ..Default::default() };
    let network = args.network();
    let params: Params = network.into();

    let daemon = Daemon::new_random_with_args(args);
    let (workers, client) = daemon.start().await;

    //
    //
    //

    /*
       1. use the new feature for preallocating utxos
       2. set up a dataset with a DAG of signed txs over the preallocated utxoset
       3. create constant mempool pressure by submitting txs (via rpc for now)
       4. mine to the node
       5. measure bbt latency, real-time bps, real-time throughput, mempool draining rate
    */

    /*
       Goals:
           call bbt every 1/bps on average
           find a block in the expected time
           submit the block with delay
           skip pow
           so no need for discrete event simulator since we simulate in real-time
    */

    let dist: Exp<f64> = Exp::new(params.bps() as f64).unwrap();
    // let delay = Duration::from_millis(1000);

    let (sk, pk) = &secp256k1::generate_keypair(&mut thread_rng());
    let pay_address =
        Address::new(network.network_type().into(), kaspa_addresses::Version::PubKey, &pk.x_only_public_key().0.serialize());
    debug!("Generated private key {} and address {}", sk.display_secret(), pay_address);

    for _ in 0..100 {
        let bbt = client.get_block_template(pay_address.clone(), vec![]).await.unwrap();
        // Simulate mining time
        let timeout = max((dist.sample(&mut thread_rng()) * 1000.0) as u64, 1);
        tokio::time::sleep(Duration::from_millis(timeout)).await;
        let response = client.submit_block(bbt.block, false).await.unwrap();
        assert_eq!(response.report, kaspa_rpc_core::SubmitBlockReport::Success);
    }

    //
    // Fold-up
    //
    // tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    client.disconnect().await.unwrap();
    drop(client);
    daemon.core.shutdown();
    daemon.core.join(workers);
}
