use crate::common::daemon::Daemon;
use async_channel::Sender;
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_consensus::params::Params;
use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION},
    network::NetworkType,
    sign::sign,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ScriptPublicKey, SignableTransaction, Transaction, TransactionInput, TransactionOutput},
    utxo::{
        utxo_collection::{UtxoCollection, UtxoCollectionExtensions},
        utxo_diff::UtxoDiff,
    },
};
use kaspa_core::{debug, info, signals::Shutdown};
use kaspa_notify::{
    listener::ListenerId,
    notifier::Notify,
    scope::{NewBlockTemplateScope, Scope},
};
use kaspa_rpc_core::{api::rpc::RpcApi, Notification, RpcError};
use kaspa_txscript::pay_to_address_script;
use kaspad::args::Args;
use parking_lot::Mutex;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use secp256k1::KeyPair;
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

const FEE_PER_MASS: u64 = 10;

fn required_fee(num_inputs: usize, num_outputs: u64) -> u64 {
    FEE_PER_MASS * estimated_mass(num_inputs, num_outputs)
}

fn estimated_mass(num_inputs: usize, num_outputs: u64) -> u64 {
    200 + 34 * num_outputs + 1000 * (num_inputs as u64)
}

fn generate_tx_dag(mut utxoset: UtxoCollection, schnorr_key: KeyPair, spk: ScriptPublicKey) -> Vec<Arc<Transaction>> {
    /*
       perform level by level:
           for target txs per level:
               select random utxos (distinctly)
               create and sign a tx
               append tx to level txs
               append tx to utxo diff
           apply level utxo diff to the utxo collection
    */

    let target_levels = 1_000;
    let target_width = 500;
    let num_inputs = 2;
    let num_outputs = 2;

    let mut txs = Vec::with_capacity(target_levels * target_width);

    for i in 0..target_levels {
        let mut utxo_diff = UtxoDiff::default();
        utxoset
            .iter()
            .take(num_inputs * target_width)
            .chunks(num_inputs)
            .into_iter()
            .map(|c| c.into_iter().map(|(o, e)| (TransactionInput::new(*o, vec![], 0, 1), e.clone())).unzip())
            .collect::<Vec<(Vec<_>, Vec<_>)>>()
            .into_par_iter()
            .map(|(inputs, entries)| {
                let total_in = entries.iter().map(|e| e.amount).sum::<u64>();
                let total_out = total_in - required_fee(num_inputs, num_outputs);
                let outputs = (0..num_outputs)
                    .map(|_| TransactionOutput { value: total_out / num_outputs, script_public_key: spk.clone() })
                    .collect_vec();
                let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
                sign(SignableTransaction::with_entries(unsigned_tx, entries), schnorr_key)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|signed_tx| {
                utxo_diff.add_transaction(&signed_tx.as_verifiable(), 0).unwrap();
                txs.push(Arc::new(signed_tx.tx));
            });
        utxoset.remove_collection(&utxo_diff.remove);
        utxoset.add_collection(&utxo_diff.add);

        if i % 100 == 0 {
            info!("Generated {} txs", txs.len());
        }
    }

    txs
}

#[tokio::test]
#[ignore = "bmk"]
async fn bench_bbt_latency() {
    kaspa_core::log::try_init_logger("info");

    let (prealloc_sk, prealloc_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let prealloc_address =
        Address::new(NetworkType::Simnet.into(), kaspa_addresses::Version::PubKey, &prealloc_pk.x_only_public_key().0.serialize());
    let schnorr_key = secp256k1::KeyPair::from_secret_key(secp256k1::SECP256K1, &prealloc_sk);
    let spk = pay_to_address_script(&prealloc_address);

    //
    // Setup
    //
    let args = Args {
        simnet: true,
        enable_unsynced_mining: true,
        num_prealloc_utxos: Some(500_000),
        prealloc_address: Some(prealloc_address.to_string()),
        prealloc_amount: 500 * SOMPI_PER_KASPA,
        ..Default::default()
    };
    let network = args.network();
    let params: Params = network.into();

    let utxoset = args.generate_prealloc_utxos(args.num_prealloc_utxos.unwrap());
    let txs = generate_tx_dag(utxoset, schnorr_key, spk);
    info!("Generated overall {} txs", txs.len());

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
        for i in 0..10000 {
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

    let cc = client.clone();
    let tx_sender_task = tokio::spawn(async move {
        for tx in txs {
            let res = cc.submit_transaction(tx.as_ref().into(), false).await;
            match res {
                Ok(_) => {}
                Err(RpcError::RejectedTransaction(_, msg)) if msg.contains("orphan") => {}
                Err(e) => panic!("{e}"),
            }
        }
    });

    let _ = join!(miner_receiver_task, miner_loop_task, tx_sender_task);

    //
    // Fold-up
    //
    // tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    client.disconnect().await.unwrap();
    drop(client);
    daemon.core.shutdown();
    daemon.core.join(workers);
}
