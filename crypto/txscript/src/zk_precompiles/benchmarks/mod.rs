#[cfg(test)]
mod test {
    use crate::{
        data_stack::Stack,
        zk_precompiles::{parse_tag, verify_zk},
    };

    #[test]
    fn test_benchmark_verification() {
        use hex::decode;
        use rand::rngs::OsRng;
        use secp256k1::ecdsa::Signature;
        use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
        use std::time::{Duration, Instant};

        // Load the STARK proof hex from files
        let stark_proof_hex = include_str!("succinct.proof.hex");
        let stark_proof_bytes = decode(stark_proof_hex).expect("Failed to decode hex STARK proof");
        let stark_image_id_hex = include_str!("succinct.image.hex");
        let stark_image_id_bytes = decode(stark_image_id_hex).expect("Failed to decode hex image id");
        let stark_journal_hex = include_str!("succinct.journal.hex");
        let stark_journal_bytes = decode(stark_journal_hex).expect("Failed to decode hex journal");

        // Hardcoded Groth16 test data
        let unprepared_compressed_vk = decode("e2f26dbea299f5223b646cb1fb33eadb059d9407559d7441dfd902e3a79a4d2dabb73dc17fbc13021e2471e0c08bd67d8401f52b73d6d07483794cad4778180e0c06f33bbc4c79a9cadef253a68084d382f17788f885c9afd176f7cb2f036789edf692d95cbdde46ddda5ef7d422436779445c5e66006a42761e1f12efde0018c212f3aeb785e49712e7a9353349aaf1255dfb31b7bf60723a480d9293938e1933033e7fea1f40604eaacf699d4be9aacc577054a0db22d9129a1728ff85a01a1c3af829b62bf4914c0bcf2c81a4bd577190eff5f194ee9bac95faefd53cb0030600000000000000e43bdc655d0f9d730535554d9caa611ddd152c081a06a932a8e1d5dc259aac123f42a188f683d869873ccc4c119442e57b056e03e2fa92f2028c97bc20b9078747c30f85444697fdf436e348711c011115963f855197243e4b39e6cbe236ca8ba7f2042e11f9255afbb6c6e2c3accb88e401f2aac21c097c92b3fbdb99f98a9b0dcd6c075ada6ed0ddfece1d4a2d005f61a7d5df0b75c18a5b2374d64e495fab93d4c4b1200394d5253cce2f25a59b862ee8e4cd43686603faa09d5d0d3c1c8f").unwrap();
        let groth16_proof_bytes = decode("570253c0c483a1b16460118e63c155f3684e784ae7d97e8fc3f544128b37fe15075eab5ac31150c8a44253d8525971241bbd7227fcefbae2db4ae71675c56a2e0eb9235136b15ab72f16e707832f3d6ae5b0ba7cca53ae17cb52b3201919eb9d908c16297abd90aa7e00267bc21a9a78116e717d4d76edd44e21cca17e3d592d").unwrap();
        let input0 = decode("a54dc85ac99f851c92d7c96d7318af4100000000000000000000000000000000").unwrap();
        let input1 = decode("dbe7c0194edfcc37eb4d422a998c1f5600000000000000000000000000000000").unwrap();
        let input2 = decode("a95ac0b37bfedcd8136e6c1143086bf500000000000000000000000000000000").unwrap();
        let input3 = decode("d223ffcb21c6ffcb7c8f60392ca49dde00000000000000000000000000000000").unwrap();
        let input4 = decode("c07a65145c3cb48b6101962ea607a4dd93c753bb26975cb47feb00d3666e4404").unwrap();

        let stark_tag = 0x21;
        let groth16_tag = 0x20;

        let stark_stack = Stack::from(vec![stark_proof_bytes, stark_journal_bytes, stark_image_id_bytes, [stark_tag].to_vec()]);

        // Build Groth16 stack with hardcoded values (matching the order from try_verify_stack test)
        let mut groth_stack = Stack::new(Vec::new(), true);
        groth_stack.push(input4).unwrap();
        groth_stack.push(input3).unwrap();
        groth_stack.push(input2).unwrap();
        groth_stack.push(input1).unwrap();
        groth_stack.push(input0).unwrap();
        groth_stack.push_item(5u16).unwrap();
        groth_stack.push(groth16_proof_bytes).unwrap();
        groth_stack.push(unprepared_compressed_vk).unwrap();
        groth_stack.push([groth16_tag].to_vec()).unwrap();

        // Prepare ECDSA verification components (outside timing loop)
        let secp = Secp256k1::new();
        let sk = SecretKey::new(&mut OsRng);
        let pk = PublicKey::from_secret_key(&secp, &sk);
        let msg_hash = [u8::MAX; 32]; // Dummy message hash
        let msg = Message::from_digest_slice(&msg_hash).expect("Failed to create message");
        let sig: Signature = secp.sign_ecdsa(&msg, &sk);

        const ITERATIONS: u32 = 1000;

        // Benchmark STARK verification
        let mut total_stark_time = Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut stark_stack_clone = stark_stack.clone();
            let start = Instant::now();
            let tag = parse_tag(&mut stark_stack_clone).unwrap();
            verify_zk(tag, &mut stark_stack_clone).unwrap();
            total_stark_time += start.elapsed();
        }
        let avg_stark_time = total_stark_time / ITERATIONS;

        // Benchmark Groth16 verification
        let mut total_groth16_time = Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut groth_stack_clone = groth_stack.clone();
            let start = Instant::now();
            let tag = parse_tag(&mut groth_stack_clone).unwrap();
            verify_zk(tag, &mut groth_stack_clone).unwrap();
            total_groth16_time += start.elapsed();
        }
        let avg_groth16_time = total_groth16_time / ITERATIONS;

        // Benchmark ECDSA signature verification
        let mut total_sig_time = Duration::ZERO;
        for _ in 0..ITERATIONS {
            let start = Instant::now();
            secp.verify_ecdsa(&msg, &sig, &pk).expect("Signature verification failed");
            total_sig_time += start.elapsed();
        }
        let avg_sig_time = total_sig_time / ITERATIONS;

        // Output the comparison
        println!("\n=== Verification Benchmark Results ({} iterations) ===", ITERATIONS);
        println!("Average STARK verification time:   {:?}", avg_stark_time);
        println!("Average Groth16 verification time: {:?}", avg_groth16_time);
        println!("Average ECDSA verification time:   {:?}", avg_sig_time);

        println!("\n=== Relative Performance ===");
        println!(
            "STARK is {:.2}x {} than ECDSA",
            if avg_stark_time > avg_sig_time {
                avg_stark_time.as_secs_f64() / avg_sig_time.as_secs_f64()
            } else {
                avg_sig_time.as_secs_f64() / avg_stark_time.as_secs_f64()
            },
            if avg_stark_time > avg_sig_time { "slower" } else { "faster" }
        );
        println!(
            "Groth16 is {:.2}x {} than ECDSA",
            if avg_groth16_time > avg_sig_time {
                avg_groth16_time.as_secs_f64() / avg_sig_time.as_secs_f64()
            } else {
                avg_sig_time.as_secs_f64() / avg_groth16_time.as_secs_f64()
            },
            if avg_groth16_time > avg_sig_time { "slower" } else { "faster" }
        );
        println!(
            "STARK is {:.2}x {} than Groth16",
            if avg_stark_time > avg_groth16_time {
                avg_stark_time.as_secs_f64() / avg_groth16_time.as_secs_f64()
            } else {
                avg_groth16_time.as_secs_f64() / avg_stark_time.as_secs_f64()
            },
            if avg_stark_time > avg_groth16_time { "slower" } else { "faster" }
        );
    }

    #[test]
    #[ignore = "long-running benchmark test"]
    fn test_batch_verification_parallelism() {
        use hex::decode;
        use rayon::ThreadPoolBuilder;
        use rayon::prelude::*;
        use std::time::Instant;

        // Load STARK proof
        let stark_proof_hex = include_str!("succinct.proof.hex");
        let stark_proof_bytes = decode(stark_proof_hex).expect("Failed to decode hex STARK proof");
        let stark_image_id_hex = include_str!("succinct.image.hex");
        let stark_image_id_bytes = decode(stark_image_id_hex).expect("Failed to decode hex image id");
        let stark_journal_hex = include_str!("succinct.journal.hex");
        let stark_journal_bytes = decode(stark_journal_hex).expect("Failed to decode hex journal");
        let stark_tag = 0x21;

        let stark_stack = Stack::from(vec![stark_proof_bytes, stark_journal_bytes, stark_image_id_bytes, [stark_tag].to_vec()]);

        // Create batch of proofs
        const BATCH_SIZE: usize = 50;
        let proof_batch: Vec<_> = (0..BATCH_SIZE).map(|_| stark_stack.clone()).collect();

        println!("\n=== Test 2: Batch Verification Parallelism ({} STARK Proofs) ===", BATCH_SIZE);

        let mut baseline_time = None;

        for num_threads in [1, 2, 4, 8, 16] {
            let pool = ThreadPoolBuilder::new().num_threads(num_threads).build().unwrap();

            // Warmup
            pool.install(|| {
                proof_batch.par_iter().all(|stack| {
                    let mut s = stack.clone();
                    let tag = parse_tag(&mut s).unwrap();
                    verify_zk(tag, &mut s).is_ok()
                });
            });

            // Benchmark with high iteration count for stable results
            const ITERATIONS: u32 = 100;
            let mut times = Vec::new();

            for _ in 0..ITERATIONS {
                let start = Instant::now();

                pool.install(|| {
                    proof_batch.par_iter().all(|stack| {
                        let mut s = stack.clone();
                        let tag = parse_tag(&mut s).unwrap();
                        verify_zk(tag, &mut s).is_ok()
                    })
                });

                times.push(start.elapsed());
            }

            // Calculate mean
            let mean = times.iter().sum::<std::time::Duration>() / ITERATIONS;
            let mean_ms = mean.as_secs_f64() * 1000.0;
            let proofs_per_sec = (BATCH_SIZE as f64) / mean.as_secs_f64();

            // Calculate standard deviation
            let mean_secs = mean.as_secs_f64();
            let variance: f64 = times
                .iter()
                .map(|t| {
                    let diff = t.as_secs_f64() - mean_secs;
                    diff * diff
                })
                .sum::<f64>()
                / ITERATIONS as f64;
            let std_dev_ms = variance.sqrt() * 1000.0;

            // Calculate min/max
            let min_ms = times.iter().min().unwrap().as_secs_f64() * 1000.0;
            let max_ms = times.iter().max().unwrap().as_secs_f64() * 1000.0;

            if baseline_time.is_none() {
                baseline_time = Some(mean);
            }

            let speedup = baseline_time.unwrap().as_secs_f64() / mean.as_secs_f64();
            let efficiency = (speedup / num_threads as f64) * 100.0;

            println!(
                "{:2} threads: {:7.2}ms ± {:5.2}ms (min: {:6.2}ms, max: {:6.2}ms, speedup: {:.2}x, efficiency: {:.1}%, {:.0} proofs/sec)",
                num_threads, mean_ms, std_dev_ms, min_ms, max_ms, speedup, efficiency, proofs_per_sec
            );
        }
    }
}
