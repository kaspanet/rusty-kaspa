#[cfg(test)]
mod test {
    use crate::{data_stack::Stack, zk_precompiles::verify_zk};
    
    #[test]
    fn test_benchmark_verification() {
        use hex::decode;
        use rand::rngs::OsRng;
        use secp256k1::ecdsa::Signature;
        use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
        use std::time::{Duration, Instant};

        // Load the STARK proof hex
        let stark_proof_hex = include_str!("succinct.proof.hex");
        let stark_proof_bytes = decode(stark_proof_hex).expect("Failed to decode hex STARK proof");
        let stark_image_id_hex = include_str!("succinct.image.hex");
        let stark_image_id_bytes = decode(stark_image_id_hex).expect("Failed to decode hex image id");
        let stark_journal_hex = include_str!("succinct.journal.hex");
        let stark_journal_bytes = decode(stark_journal_hex).expect("Failed to decode hex journal");

        // Load the Groth16 proof hex
        let groth16_proof_hex = include_str!("groth.proof.hex");
        let groth16_proof_bytes = decode(groth16_proof_hex).expect("Failed to decode hex groth16 proof");
        let groth16_image_id_hex = include_str!("groth.image.hex");
        let groth16_image_id_bytes = decode(groth16_image_id_hex).expect("Failed to decode hex image id");
        let groth16_journal_hex = include_str!("groth.journal.hex");
        let groth16_journal_bytes = decode(groth16_journal_hex).expect("Failed to decode hex journal");

        let stark_tag = 0x21;
        let groth16_tag = 0x20;

        let stark_stack = Stack::from(vec![stark_proof_bytes, stark_journal_bytes, stark_image_id_bytes, [stark_tag].to_vec()]);

        let groth_stack =
            Stack::from(vec![groth16_proof_bytes, groth16_journal_bytes, groth16_image_id_bytes, [groth16_tag].to_vec()]);

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
            verify_zk(&mut stark_stack_clone).unwrap();
            total_stark_time += start.elapsed();
        }
        let avg_stark_time = total_stark_time / ITERATIONS;

        // Benchmark Groth16 verification
        let mut total_groth16_time = Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut groth_stack_clone = groth_stack.clone();
            let start = Instant::now();
            verify_zk(&mut groth_stack_clone).unwrap();
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
    fn test_batch_verification_parallelism() {
        use hex::decode;
        use rayon::prelude::*;
        use rayon::ThreadPoolBuilder;
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
            let pool = ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .unwrap();

            // Warmup
            pool.install(|| {
                proof_batch.par_iter().all(|stack| {
                    let mut s = stack.clone();
                    verify_zk(&mut s).is_ok()
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
                        verify_zk(&mut s).is_ok()
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
            let variance: f64 = times.iter()
                .map(|t| {
                    let diff = t.as_secs_f64() - mean_secs;
                    diff * diff
                })
                .sum::<f64>() / ITERATIONS as f64;
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