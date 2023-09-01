use crate::common::daemon::Daemon;
use async_channel::Sender;
use kaspa_addresses::Address;
use kaspa_consensus::params::Params;
use kaspa_core::{debug, signals::Shutdown};
use kaspa_notify::{
    listener::ListenerId,
    notifier::Notify,
    scope::{NewBlockTemplateScope, Scope},
};
use kaspa_rpc_core::{api::rpc::RpcApi, Notification};
use kaspad::args::Args;
use parking_lot::Mutex;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use std::{cmp::max, fmt::Debug, sync::Arc, time::Duration};
use tokio::join;

#[derive(Debug)]
struct ChannelNotify {
    sender: Sender<Notification>,
}

impl Notify<Notification> for ChannelNotify {
    fn notify(&self, notification: Notification) -> kaspa_notify::error::Result<()> {
        self.sender.try_send(notification)?;
        Ok(())
    }
}

#[tokio::test]
#[ignore = "bmk"]
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
    let miner_client = daemon.new_client().await;

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

    // The time interval between Poisson(lambda) events distributes ~Exp(lambda)
    let dist: Exp<f64> = Exp::new(params.bps() as f64).unwrap();
    let comm_delay = 1000;

    let (sk, pk) = &secp256k1::generate_keypair(&mut thread_rng());
    let pay_address =
        Address::new(network.network_type().into(), kaspa_addresses::Version::PubKey, &pk.x_only_public_key().0.serialize());
    debug!("Generated private key {} and address {}", sk.display_secret(), pay_address);

    let current_template = Arc::new(Mutex::new(miner_client.get_block_template(pay_address.clone(), vec![]).await.unwrap()));
    let current_template_consume = current_template.clone();

    let (sender, receiver) = async_channel::unbounded();
    miner_client.start(Some(Arc::new(ChannelNotify { sender }))).await;
    miner_client.start_notify(ListenerId::default(), Scope::NewBlockTemplate(NewBlockTemplateScope {})).await.unwrap();

    let mcc = miner_client.clone();
    let miner_receiver_task = tokio::spawn(async move {
        while let Ok(notification) = receiver.recv().await {
            match notification {
                Notification::NewBlockTemplate(_) => {
                    while receiver.try_recv().is_ok() {
                        // Drain the channel
                    }
                    *current_template.lock() = mcc.get_block_template(pay_address.clone(), vec![]).await.unwrap();
                }
                _ => panic!(),
            }
        }
    });

    let miner_loop_task = tokio::spawn(async move {
        for i in 0..1000 {
            // Simulate mining time
            let timeout = max((dist.sample(&mut thread_rng()) * 1000.0) as u64, 1);
            tokio::time::sleep(Duration::from_millis(timeout)).await;

            // Read the most up-to-date block template
            let mut block = current_template_consume.lock().block.clone();
            // Use index as nonce to avoid duplicate blocks
            block.header.nonce = i;

            let mcc = miner_client.clone();
            tokio::spawn(async move {
                // Simulate communication delay. TODO: consider adding gaussian noise
                tokio::time::sleep(Duration::from_millis(comm_delay)).await;
                let response = mcc.submit_block(block, false).await.unwrap();
                assert_eq!(response.report, kaspa_rpc_core::SubmitBlockReport::Success);
            });
        }
        miner_client.disconnect().await.unwrap();
    });

    let _ = join!(miner_receiver_task, miner_loop_task);

    //
    // Fold-up
    //
    // tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    client.disconnect().await.unwrap();
    drop(client);
    daemon.core.shutdown();
    daemon.core.join(workers);
}
