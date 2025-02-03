use criterion::{black_box, criterion_group, criterion_main, Criterion, SamplingMode};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus::processes::transaction_validator::tx_validation_in_utxo_context::{
    check_scripts_par_iter, check_scripts_par_iter_pool, check_scripts_sequential,
};
use kaspa_consensus_core::hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, UtxoEntry};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::pay_to_address_script;
use kaspa_utils::iter::parallelism_in_power_steps;
use rand::{thread_rng, Rng};
use secp256k1::Keypair;

fn mock_tx_with_payload(inputs_count: usize, non_uniq_signatures: usize, payload_size: usize) -> (Transaction, Vec<UtxoEntry>) {
    let mut payload = vec![0u8; payload_size];
    thread_rng().fill(&mut payload[..]);

    let reused_values = SigHashReusedValuesUnsync::new();
    let dummy_prev_out = TransactionOutpoint::new(kaspa_hashes::Hash::from_u64_word(1), 1);
    let mut tx = Transaction::new(
        0,
        vec![],
        vec![],
        0,
        SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        0,
        payload,
    );
    let mut utxos = vec![];
    let mut kps = vec![];

    for _ in 0..inputs_count - non_uniq_signatures {
        let kp = Keypair::new(secp256k1::SECP256K1, &mut thread_rng());
        tx.inputs.push(TransactionInput { previous_outpoint: dummy_prev_out, signature_script: vec![], sequence: 0, sig_op_count: 1 });
        let address = Address::new(Prefix::Mainnet, Version::PubKey, &kp.x_only_public_key().0.serialize());
        utxos.push(UtxoEntry {
            amount: thread_rng().gen::<u32>() as u64,
            script_public_key: pay_to_address_script(&address),
            block_daa_score: 333,
            is_coinbase: false,
        });
        kps.push(kp);
    }

    for _ in 0..non_uniq_signatures {
        let kp = kps.last().unwrap();
        tx.inputs.push(TransactionInput { previous_outpoint: dummy_prev_out, signature_script: vec![], sequence: 0, sig_op_count: 1 });
        let address = Address::new(Prefix::Mainnet, Version::PubKey, &kp.x_only_public_key().0.serialize());
        utxos.push(UtxoEntry {
            amount: thread_rng().gen::<u32>() as u64,
            script_public_key: pay_to_address_script(&address),
            block_daa_score: 444,
            is_coinbase: false,
        });
    }

    for (i, kp) in kps.iter().enumerate().take(inputs_count - non_uniq_signatures) {
        let mut_tx = MutableTransaction::with_entries(&tx, utxos.clone());
        let sig_hash = calc_schnorr_signature_hash(&mut_tx.as_verifiable(), i, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
        let sig: [u8; 64] = *kp.sign_schnorr(msg).as_ref();
        tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    }

    let length = tx.inputs.len();
    for i in (inputs_count - non_uniq_signatures)..length {
        let kp = kps.last().unwrap();
        let mut_tx = MutableTransaction::with_entries(&tx, utxos.clone());
        let sig_hash = calc_schnorr_signature_hash(&mut_tx.as_verifiable(), i, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(sig_hash.as_bytes().as_slice()).unwrap();
        let sig: [u8; 64] = *kp.sign_schnorr(msg).as_ref();
        tx.inputs[i].signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
    }

    (tx, utxos)
}

fn benchmark_check_scripts(c: &mut Criterion) {
    for inputs_count in [100, 50, 25, 10, 5, 2] {
        for non_uniq_signatures in [0, inputs_count / 2] {
            let (tx, utxos) = mock_tx_with_payload(inputs_count, non_uniq_signatures, 0);
            let mut group = c.benchmark_group(format!("inputs: {inputs_count}, non uniq: {non_uniq_signatures}"));
            group.sampling_mode(SamplingMode::Flat);

            group.bench_function("single_thread", |b| {
                let tx = MutableTransaction::with_entries(&tx, utxos.clone());
                let cache = Cache::new(inputs_count as u64);
                b.iter(|| {
                    cache.clear();
                    check_scripts_sequential(black_box(&cache), black_box(&tx.as_verifiable()), false, false).unwrap();
                })
            });

            group.bench_function("rayon par iter", |b| {
                let tx = MutableTransaction::with_entries(tx.clone(), utxos.clone());
                let cache = Cache::new(inputs_count as u64);
                b.iter(|| {
                    cache.clear();
                    check_scripts_par_iter(black_box(&cache), black_box(&tx.as_verifiable()), false, false).unwrap();
                })
            });

            for i in parallelism_in_power_steps() {
                if inputs_count >= i {
                    group.bench_function(format!("rayon, custom thread pool, thread count {i}"), |b| {
                        let tx = MutableTransaction::with_entries(tx.clone(), utxos.clone());
                        let pool = rayon::ThreadPoolBuilder::new().num_threads(i).build().unwrap();
                        let cache = Cache::new(inputs_count as u64);
                        b.iter(|| {
                            cache.clear();
                            check_scripts_par_iter_pool(
                                black_box(&cache),
                                black_box(&tx.as_verifiable()),
                                black_box(&pool),
                                false,
                                false,
                            )
                            .unwrap();
                        })
                    });
                }
            }
        }
    }
}

/// Benchmarks script checking performance with different payload sizes and input counts.
///
/// This benchmark evaluates the performance impact of transaction payload size
/// on script validation, testing multiple scenarios:
///
/// * Payload sizes: 0KB, 16KB, 32KB, 64KB, 128KB
/// * Input counts: 1, 2, 10, 50 transactions
///
/// The benchmark helps understand:
/// 1. How payload size affects validation performance
/// 2. The relationship between input count and payload processing overhead
fn benchmark_check_scripts_with_payload(c: &mut Criterion) {
    let payload_sizes = [0, 16_384, 32_768, 65_536, 131_072]; // 0, 16KB, 32KB, 64KB, 128KB
    let input_counts = [1, 2, 10, 50];
    let non_uniq_signatures = 0;

    for inputs_count in input_counts {
        for &payload_size in &payload_sizes {
            let (tx, utxos) = mock_tx_with_payload(inputs_count, non_uniq_signatures, payload_size);
            let mut group = c.benchmark_group(format!("script_check/inputs_{}/payload_{}_kb", inputs_count, payload_size / 1024));
            group.sampling_mode(SamplingMode::Flat);

            group.bench_function("parallel_validation", |b| {
                let tx = MutableTransaction::with_entries(tx.clone(), utxos.clone());
                let cache = Cache::new(inputs_count as u64);
                b.iter(|| {
                    cache.clear();
                    check_scripts_par_iter(black_box(&cache), black_box(&tx.as_verifiable()), false, false).unwrap();
                })
            });
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_output_color(true).measurement_time(std::time::Duration::new(20, 0));
    targets = benchmark_check_scripts, benchmark_check_scripts_with_payload
}

criterion_main!(benches);
