use ark_snark::SNARK;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use kaspa_consensus_core::{
    hashing::sighash::{SigHashReusedValuesSync, SigHashReusedValuesUnsync},
    tx::PopulatedTransaction,
};
use kaspa_txscript::{
    EngineFlags, TxScriptEngine,
    caches::Cache,
    zk_precompiles::tests::helpers::{build_groth_script, build_stark_script, execute_zk_script},
};
use rand::rngs::OsRng;
use secp256k1::ecdsa::Signature;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};

fn benchmark_zk_precompiles(c: &mut Criterion) {
    let stark_script = build_stark_script();
    let groth_script = build_groth_script();

    let sig_cache = Cache::new(0);
    let reused_values = SigHashReusedValuesUnsync::new();

    let secp = Secp256k1::new();
    let sk = SecretKey::new(&mut OsRng);
    let pk = PublicKey::from_secret_key(&secp, &sk);
    let msg_hash = [u8::MAX; 32];
    let msg = Message::from_digest_slice(&msg_hash).expect("Failed to create message");
    let sig: Signature = secp.sign_ecdsa(&msg, &sk);

    let mut group = c.benchmark_group("zk_precompile_verification");
    group.bench_function("r0_succinct", |b| {
        b.iter(|| execute_zk_script(black_box(&stark_script), &sig_cache, &reused_values).unwrap())
    });
    group.bench_function("groth16", |b| b.iter(|| execute_zk_script(black_box(&groth_script), &sig_cache, &reused_values).unwrap()));
    group.bench_function("ecdsa", |b| b.iter(|| secp.verify_ecdsa(&msg, &sig, &pk).expect("Signature verification failed")));
    group.finish();
}

fn benchmark_r0_batch_parallelism(c: &mut Criterion) {
    use rayon::ThreadPoolBuilder;
    use rayon::prelude::*;

    const BATCH_SIZE: usize = 50;
    let stark_script = build_stark_script();
    let proof_batch: Vec<_> = (0..BATCH_SIZE).map(|_| stark_script.clone()).collect();

    let mut group = c.benchmark_group("r0_batch_parallelism");
    for num_threads in [1usize, 2, 4, 8, 16] {
        let pool = ThreadPoolBuilder::new().num_threads(num_threads).build().unwrap();
        group.bench_function(format!("threads_{num_threads}"), |b| {
            b.iter(|| {
                let cache = Cache::new(0);
                let reused_values = SigHashReusedValuesSync::new();
                pool.install(|| {
                    proof_batch.par_iter().all(|script| {
                        let mut vm = TxScriptEngine::<PopulatedTransaction, SigHashReusedValuesSync>::from_script(
                            script,
                            &reused_values,
                            &cache,
                            EngineFlags { covenants_enabled: true },
                        );
                        vm.execute().is_ok()
                    })
                });
            })
        });
    }
    group.finish();
}

fn benchmark_groth16_prepare_inputs(c: &mut Criterion) {
    use ark_bn254::{Bn254, Fr};
    use ark_groth16::Groth16;
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};

    struct Circuit {
        num_public_inputs: usize,
    }

    impl ConstraintSynthesizer<Fr> for Circuit {
        fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
            let mut running_sum = 0u64;
            let mut sum_var = cs.new_witness_variable(|| Ok(Fr::from(0u64)))?;
            for i in 0..self.num_public_inputs {
                let input = cs.new_input_variable(|| Ok(Fr::from(i as u64)))?;
                running_sum += i as u64;
                let new_sum_var = cs.new_witness_variable(|| Ok(Fr::from(running_sum)))?;
                let one = ark_relations::r1cs::Variable::One;
                cs.enforce_constraint(
                    ark_relations::lc!() + sum_var + input,
                    ark_relations::lc!() + one,
                    ark_relations::lc!() + new_sum_var,
                )?;
                sum_var = new_sum_var;
            }
            Ok(())
        }
    }

    let test_sizes = [10_000, 20_000, 40_000, 80_000, 160_000];
    let mut group = c.benchmark_group("groth16_prepare_inputs");

    for num_inputs in test_sizes {
        let mut rng = rand::thread_rng();
        let circuit = Circuit { num_public_inputs: num_inputs };
        let (_, vk) = Groth16::<Bn254>::circuit_specific_setup(circuit, &mut rng).expect("Setup failed");
        let pvk = ark_groth16::prepare_verifying_key(&vk);
        let public_inputs: Vec<Fr> = (0..num_inputs).map(|i| Fr::from(i as u64)).collect();

        group.bench_function(format!("inputs_{num_inputs}"), |b| {
            b.iter(|| {
                let _ = Groth16::<Bn254>::prepare_inputs(&pvk, black_box(&public_inputs)).unwrap();
            })
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_output_color(true);
    targets = benchmark_zk_precompiles, benchmark_r0_batch_parallelism, benchmark_groth16_prepare_inputs
}

criterion_main!(benches);
