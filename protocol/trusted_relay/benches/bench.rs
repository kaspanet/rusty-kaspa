use std::time::{Duration, Instant};

use bytes::Bytes;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use hmac::{Hmac, Mac};
use poly1305::universal_hash::UniversalHash;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

use kaspa_consensus_core::Hash;
use kaspa_trusted_relay::ftr_block::FtrBlock;
use kaspa_trusted_relay::fragmentation::config::ShardingConfig;
use kaspa_trusted_relay::fragmentation::decoding::block_reassembler::{Coordinator, DecoderConfig};
use kaspa_trusted_relay::fragmentation::encoder::ShardGenerator;
use kaspa_trusted_relay::fragmentation::fragments::{Shard, ShardHeader};

/// Enlarge the kernel UDP receive buffer on a std socket.
/// Uses raw setsockopt on Unix; no-op on other platforms.
#[cfg(unix)]
fn set_recv_buf(sock: &std::net::UdpSocket, size: usize) {
    socket2::SockRef::from(sock).set_recv_buffer_size(size).expect("Failed to set SO_RCVBUF");
    socket2::SockRef::from(sock).set_send_buffer_size(size / 2).expect("Failed to set SO_SNDBUF");
    socket2::SockRef::from(sock).set_nonblocking(false).expect("Failed to set non-blocking mode");
}

#[cfg(not(unix))]
fn set_recv_buf(_sock: &std::net::UdpSocket, _size: usize) {}

/// Minimal FFI shim — avoids pulling in the full `libc` crate just for one
/// setsockopt call in a benchmark.
#[cfg(unix)]
mod libc_shim {
    pub type c_int = i32;
    const SOL_SOCKET: c_int = 1;
    const SO_RCVBUF: c_int = 8;

    /// Set SO_RCVBUF on a raw file descriptor.
    pub unsafe fn setsockopt_rcvbuf(fd: c_int, val: &c_int) {
        unsafe {
            libc_setsockopt(
                fd,
                SOL_SOCKET,
                SO_RCVBUF,
                val as *const c_int as *const core::ffi::c_void,
                core::mem::size_of::<c_int>() as u32,
            );
        }
    }

    unsafe extern "C" {
        #[link_name = "setsockopt"]
        fn libc_setsockopt(socket: c_int, level: c_int, name: c_int, value: *const core::ffi::c_void, option_len: u32) -> c_int;
    }
}

fn benchmark_shard_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_generation");

    // Test different data sizes
    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("500KB", 500 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            b.iter(|| {
                let test_data = black_box(vec![42u8; size]);
                let config = ShardingConfig::new(4, 2, 1200);
                let hash = Hash::from([1u8; 32]);

                let generator = ShardGenerator::new(config, hash, FtrBlock(test_data));
                let _shards: Vec<_> = black_box(generator.collect());
            });
        });
    }

    group.finish();
}

fn benchmark_shard_generation_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_generation_small");

    // Single generation benchmarks
    let config = ShardingConfig::new(4, 2, 1200);
    let single_gen_size = 4 * 1200; // One full generation

    group.bench_function("single_generation", |b| {
        b.iter(|| {
            let test_data = black_box(vec![42u8; single_gen_size]);
            let hash = Hash::from([2u8; 32]);

            let generator = ShardGenerator::new(config, hash, FtrBlock(test_data));
            let _shards: Vec<_> = black_box(generator.collect());
        });
    });

    group.finish();
}

fn benchmark_different_configs(c: &mut Criterion) {
    let mut group = c.benchmark_group("fec_configs");
    let test_data = vec![42u8; 500 * 1024];

    let configs = vec![
        ("k4_m1", 4, 1),
        ("k4_m2", 4, 2),
        ("k4_m4", 4, 4),
        ("k16_m4", 16, 4),
        ("k16_m8", 16, 8),
        ("k16_m16", 16, 16),
        ("k32_m8", 32, 8),
        ("k32_m16", 32, 16),
        ("k32_m32", 32, 32),
    ];

    for (label, k, m) in configs {
        group.bench_with_input(BenchmarkId::from_parameter(label), &(k, m), |b, &(k, m)| {
            b.iter(|| {
                let config = ShardingConfig::new(k, m, 1200);
                let hash = Hash::from([3u8; 32]);

                let generator = ShardGenerator::new(config, hash, FtrBlock(black_box(test_data.clone())));
                let _shards: Vec<_> = black_box(generator.collect());
            });
        });
    }

    group.finish();
}

// ============================================================================
// DECODING BENCHMARKS — measures only shard send + coordinator decode + receive.
// Coordinator creation, worker thread spawning, and FEC encoding are all
// performed outside the timed region.
// ============================================================================

/// Build a unique hash for each benchmark iteration so the Coordinator's
/// processed-block cache never rejects a repeat.
#[inline]
fn iter_hash(prefix: u8, i: u64) -> Hash {
    let mut bytes = [prefix; 32];
    bytes[..8].copy_from_slice(&i.to_le_bytes());
    Hash::from_bytes(bytes)
}

/// Encode → feed all shards (perfect reception) → decode.
fn benchmark_decoding_perfect_reception(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoding_perfect");

    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("500KB", 500 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            let config = ShardingConfig::new(16, 4, 1200);
            let test_data = vec![42u8; size];

            b.iter_custom(|iters| {
                // Setup (not measured): create coordinator once for all iterations.
                let (coordinator, shard_tx, block_rx) = Coordinator::new(config, DecoderConfig::default()).unwrap();
                let handle = std::thread::spawn(move || coordinator.run());

                let mut total = Duration::ZERO;
                for i in 0..iters {
                    // Encode outside measurement.
                    let hash = iter_hash(0xAA, i);
                    let shards: Vec<Shard> = ShardGenerator::new(config, hash, FtrBlock(black_box(test_data.clone()))).collect();

                    // Measure only: send shards → coordinator decode → receive block.
                    let start = Instant::now();
                    for shard in black_box(shards) {
                        shard_tx.send(shard).unwrap();
                    }
                    let (_h, data) = block_rx.recv().unwrap();
                    total += start.elapsed();
                    black_box(data);
                }

                // Teardown (not measured): shut down coordinator.
                drop(shard_tx);
                handle.join().unwrap();
                total
            });
        });
    }

    group.finish();
}

/// Encode → drop all parity shards (data-only fast path) → decode.
fn benchmark_decoding_fast_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoding_fast_path");

    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            let config = ShardingConfig::new(16, 4, 1200);
            let test_data = vec![42u8; size];

            b.iter_custom(|iters| {
                let (coordinator, shard_tx, block_rx) = Coordinator::new(config, DecoderConfig::default()).unwrap();
                let handle = std::thread::spawn(move || coordinator.run());

                let mut total = Duration::ZERO;
                for i in 0..iters {
                    let hash = iter_hash(0xBB, i);
                    // Keep only data shards — triggers the fast path in workers.
                    let shards: Vec<Shard> = ShardGenerator::new(config, hash, FtrBlock(black_box(test_data.clone())))
                        .filter(|s| s.header().is_data(config.data_blocks as u16, config.parity_blocks as u16))
                        .collect();

                    let start = Instant::now();
                    for shard in black_box(shards) {
                        shard_tx.send(shard).unwrap();
                    }
                    let (_h, data) = block_rx.recv().unwrap();
                    total += start.elapsed();
                    black_box(data);
                }

                drop(shard_tx);
                handle.join().unwrap();
                total
            });
        });
    }

    group.finish();
}

/// Encode → drop one data shard per generation (forces RS decode) → decode.
fn benchmark_decoding_with_loss(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoding_with_loss");

    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            let config = ShardingConfig::new(4, 2, 1200);
            let gen_size = config.shards_per_generation() as u16;
            let test_data = vec![42u8; size];

            b.iter_custom(|iters| {
                let (coordinator, shard_tx, block_rx) = Coordinator::new(config, DecoderConfig::default()).unwrap();
                let handle = std::thread::spawn(move || coordinator.run());

                let mut total = Duration::ZERO;
                for i in 0..iters {
                    let hash = iter_hash(0xCC, i);
                    // Drop the first data shard of each generation → forces RS slow path.
                    let shards: Vec<Shard> = ShardGenerator::new(config, hash, FtrBlock(black_box(test_data.clone())))
                        .filter(|s| s.header().index_within_generation(gen_size) != 0)
                        .collect();

                    let start = Instant::now();
                    for shard in black_box(shards) {
                        shard_tx.send(shard).unwrap();
                    }
                    let (_h, data) = block_rx.recv().unwrap();
                    total += start.elapsed();
                    black_box(data);
                }

                drop(shard_tx);
                handle.join().unwrap();
                total
            });
        });
    }

    group.finish();
}

/// Vary k/m ratios with a fixed 500KB block.
fn benchmark_decoding_different_k_m(c: &mut Criterion) {
    let mut group = c.benchmark_group("decoding_configs");
    let test_data = vec![42u8; 500 * 1024];

    let configs = vec![
        ("k4_m1", 4, 1),
        ("k4_m2", 4, 2),
        ("k4_m4", 4, 4),
        ("k16_m4", 16, 4),
        ("k16_m8", 16, 8),
        ("k16_m16", 16, 16),
        ("k32_m8", 32, 8),
        ("k32_m16", 32, 16),
        ("k32_m32", 32, 32),
    ];

    for (label, k, m) in configs {
        group.bench_with_input(BenchmarkId::from_parameter(label), &(k, m), |b, &(k, m)| {
            let config = ShardingConfig::new(k, m, 1200);

            b.iter_custom(|iters| {
                let (coordinator, shard_tx, block_rx) = Coordinator::new(config, DecoderConfig::default()).unwrap();
                let handle = std::thread::spawn(move || coordinator.run());

                let mut total = Duration::ZERO;
                for i in 0..iters {
                    let hash = iter_hash(0xDD, i);
                    let shards: Vec<Shard> = ShardGenerator::new(config, hash, FtrBlock(black_box(test_data.clone()))).collect();

                    let start = Instant::now();
                    for shard in black_box(shards) {
                        shard_tx.send(shard).unwrap();
                    }
                    let (_h, data) = block_rx.recv().unwrap();
                    total += start.elapsed();
                    black_box(data);
                }

                drop(shard_tx);
                handle.join().unwrap();
                total
            });
        });
    }

    group.finish();
}

/// Measure throughput of shard serialization (to_bytes) and deserialization (from Bytes).
fn benchmark_shard_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_serialization");
    let hash = Hash::from_bytes([0xEE; 32]);
    let header = ShardHeader::new(hash, 42, 100);
    let payload = Bytes::from(vec![0xAB; 1200]);
    let shard = Shard::new(header, payload);
    let wire = shard.to_bytes();

    group.bench_function("to_bytes", |b| {
        b.iter(|| {
            black_box(shard.to_bytes());
        });
    });

    group.bench_function("from_bytes", |b| {
        b.iter(|| {
            black_box(Shard::from(wire.clone()));
        });
    });

    group.finish();
}

/// Compare HMAC-SHA256, blake2b-simd, blake3, and Poly1305 for representative
/// packet sizes (measures the per-packet authentication cost used by the relay).
fn benchmark_mac_algorithms(c: &mut Criterion) {
    let mut group = c.benchmark_group("mac_algorithms");
    let secret = [0xABu8; 32];

    let sizes = [64usize, 256usize, 1024usize];

    for &size in &sizes {
        let data = vec![0x55u8; size];

        // HMAC-SHA256 (current)
        group.bench_with_input(BenchmarkId::new("hmac_sha256", size), &size, |b, _| {
            b.iter(|| {
                let mut mac = <HmacSha256 as Mac>::new_from_slice(black_box(&secret)).expect("HMAC init");
                mac.update(&data);
                black_box(mac.finalize().into_bytes());
            });
        });

        // blake2b-simd (standard, no key)
        group.bench_with_input(BenchmarkId::new("blake2b_simd", size), &size, |b, _| {
            b.iter(|| {
                let h = blake2b_simd::blake2b(&data);
                black_box(h.as_bytes());
            });
        });

        // blake3 (keyed)
        group.bench_with_input(BenchmarkId::new("blake3_keyed", size), &size, |b, _| {
            b.iter(|| {
                let keyed = blake3::keyed_hash(&secret, &data);
                keyed.as_bytes();
            });
        });

        // ChaCha20-Poly1305 (AEAD cipher with auth)
        group.bench_with_input(BenchmarkId::new("chacha20poly1305", size), &size, |b, _| {
            b.iter(|| {
                let key = Key::from(secret);
                let nonce_bytes: [u8; 12] = *b"nonce12bytes"; // exactly 12 bytes
                let nonce = Nonce::from(nonce_bytes);
                let cipher = ChaCha20Poly1305::new(&key);
                // Encrypt includes authentication (Poly1305 MAC)
                let ciphertext = cipher.encrypt(&nonce, &data[..]).expect("encryption failed");
                black_box(&ciphertext);
            });
        });

        // Poly1305 only (no encryption) - isolated MAC overhead
        group.bench_with_input(BenchmarkId::new("poly1305_only", size), &size, |b, _| {
            b.iter(|| {
                let key_bytes: [u8; 32] = black_box(secret);
                let key = poly1305::Key::from(key_bytes);
                let mut poly = poly1305::Poly1305::new(&key);
                poly.update_padded(black_box(&data));
                black_box(poly.finalize().as_slice());
            });
        });
    }

    group.finish();
}

/// Measure end-to-end UDP receive loop throughput with full pipeline.
///
/// This benchmark tests the actual UdpReceiveLoop with:
/// - MAC verification (~700ns per packet)
/// - Shard parsing and deserialization
/// - Per-worker deduplication with RingSet
/// - Distributed load across worker threads
/// - Crossbeam channel forwarding to coordinator
///
/// The tokio runtime runs in its own dedicated thread to ensure the UDP
/// receive task keeps progressing even while Criterion is iterating.
fn benchmark_udp_receive_loop(c: &mut Criterion) {
    use arc_swap::ArcSwap;
    use criterion::BatchSize;
    use kaspa_trusted_relay::auth::TokenAuthenticator;
    use kaspa_trusted_relay::params::TransportParams;
    use kaspa_trusted_relay::servers::udp_transport::ForwardJob;
    use kaspa_trusted_relay::servers::udp_transport::UdpReceiveLoop;
    use kaspa_trusted_relay::fragmentation::fragments::{Shard, ShardHeader};
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    kaspa_core::log::try_init_logger("trace");

    let mut group = c.benchmark_group("udp_receive_loop");

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());

    const NUM_PACKETS: u16 = 100;
    const PAYLOAD_SIZE: usize = 1200;

    static ITER_COUNTER: AtomicU64 = AtomicU64::new(0);

    // Set up the receive loop (OS threads) and a sender socket.
    let (send_socket, recv_addr, shard_rx, _forwarder_rx) = {
        let auth_clone = auth.clone();

        let std_recv = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind recv");
        set_recv_buf(&std_recv, 16 * 1024 * 1024);

        let recv_addr = std_recv.local_addr().expect("recv addr");

        let (shard_tx, shard_rx) = crossbeam_channel::unbounded::<Shard>();
        let shard_txs = Arc::new(vec![shard_tx]);

        let transport = TransportParams { hmac_workers: 1, default_buffer_size: 4096, ..TransportParams::default() };
        let (forwarder_tx, forwarder_rx) = crossbeam_channel::bounded::<ForwardJob>(transport.forward_channel_capacity);

        let allowlist = Arc::new(ArcSwap::from_pointee(HashSet::new()));
        let is_ready = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let config = kaspa_trusted_relay::fragmentation::config::ShardingConfig::new(16, 4, 1200);

        // Create control channels for verifiers
        let mut control_txs = Vec::with_capacity(1);
        let (control_tx, _control_rx) = crossbeam_channel::bounded(64);
        control_txs.push(control_tx);
        let control_txs = Arc::new(control_txs);

        let recv_loop = UdpReceiveLoop::new(
            Arc::new(std_recv),
            shard_txs,
            allowlist.clone(),
            auth_clone,
            forwarder_tx,
            is_ready,
            config,
            transport,
            control_txs,
        );

        let _handles = recv_loop.run();

        std::thread::sleep(Duration::from_millis(100));

        let std_send = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind send");
        let sender_addr = std_send.local_addr().expect("sender addr");
        allowlist.rcu(|old| {
            let mut s = (**old).clone();
            s.insert(sender_addr);
            s
        });

        (std_send, recv_addr, shard_rx, forwarder_rx)
    };

    group.bench_function("throughput_100_packets", |b| {
        b.iter_batched(
            || {
                //let iter_id = ITER_COUNTER.fetch_add(1, Ordering::Relaxed);
                ITER_COUNTER.fetch_add(1, Ordering::Relaxed);

                let mut frames = Vec::with_capacity(NUM_PACKETS as usize);
                for pkt_idx in 0..NUM_PACKETS {
                    let hash = Hash::from_bytes(rand::random::<[u8; 32]>());

                    let shard_idx = pkt_idx;
                    let header = ShardHeader::new(hash, shard_idx, NUM_PACKETS);
                    let shard = Shard::new(header, bytes::Bytes::from(vec![0u8; PAYLOAD_SIZE]));
                    let wire = shard.to_bytes();
                    let mac = auth.mac(&wire);

                    let mut framed = Vec::with_capacity(mac.len() + wire.len());
                    framed.extend_from_slice(&mac);
                    framed.extend_from_slice(&wire);
                    frames.push(framed);
                }

                frames
            },
            |frames| {
                // Send all frames directly via blocking send_to — no channel hop.
                for frame in black_box(&frames) {
                    //std::thread::sleep(Duration::from_micros(10));
                    let _ = send_socket.send_to(black_box(frame), black_box(recv_addr));
                }

                let mut received = 0usize;
                while received < NUM_PACKETS as usize {
                    match shard_rx.recv_timeout(Duration::from_secs(4)) {
                        Ok(_) => {
                            received += 1;
                            //eprintln!("Received packet {}/{}", received, NUM_PACKETS);
                        }
                        Err(e) => {
                            eprintln!(
                                "Timeout waiting for packets: iter{} received {}/{}: {:?}",
                                ITER_COUNTER.load(Ordering::Relaxed),
                                received,
                                NUM_PACKETS,
                                e
                            );
                            break;
                        }
                    }
                }

                assert_eq!(
                    black_box(received),
                    NUM_PACKETS as usize,
                    "Expected {} packets through pipeline, got {}",
                    NUM_PACKETS,
                    received
                );
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

// --------------------- BROADCAST WORKER BENCHMARK -------------------------

fn benchmark_broadcast_worker_e2e_1mb(c: &mut Criterion) {
    use kaspa_trusted_relay::auth::TokenAuthenticator;
    use kaspa_trusted_relay::params::TransportParams;
    use kaspa_trusted_relay::servers::tcp_control::{PeerDirection, PeerInfo};
    use kaspa_trusted_relay::servers::udp_transport::PeerDirectory;
    use kaspa_trusted_relay::fragmentation::encoder::ShardGenerator;
    use kaspa_trusted_relay::transport::BroadcastWorker;
    use std::sync::atomic::AtomicBool as StdAtomicBool;

    let mut group = c.benchmark_group("broadcast_worker");

    // bench config
    let config = kaspa_trusted_relay::fragmentation::config::ShardingConfig::new(16, 4, 1200);
    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let transport = TransportParams::default();
    let local_ready = std::sync::Arc::new(StdAtomicBool::new(true));

    // 1 MB payload (declare early so direct-send test can use it)
    let data = vec![0u8; 1024 * 1024];

    // create the socket BroadcastWorker will use (tokio::net::UdpSocket)
    let std_sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind worker socket");
    set_recv_buf(&std_sock, 16 * 1024 * 1024);
    let worker_socket = std::sync::Arc::new(std_sock.try_clone().expect("clone socket"));

    // peer receive socket
    let peer_recv = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind peer recv");
    set_recv_buf(&peer_recv, 16 * 1024 * 1024);
    // short read timeout so `recv_from` returns quickly while we poll until our
    // deadline in the benchmark loop.
    peer_recv.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
    let _ = peer_recv.set_nonblocking(false);
    let peer_addr = peer_recv.local_addr().expect("peer addr");
    eprintln!("peer_recv bound to {}", peer_addr);

    // sanity-check that a plain std UdpSocket can reach the peer recv socket
    let test_sender = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind test sender");
    test_sender.send_to(b"ping", peer_addr).expect("send ping");
    let mut ping_buf = [0u8; 16];
    let (n, src) = peer_recv.recv_from(&mut ping_buf).expect("ping recv");
    eprintln!("peer_recv ping got {} bytes from {}", n, src);
    assert_eq!(n, 4);
    assert_eq!(&ping_buf[..4], b"ping");

    // PeerDirectory with one outbound-ready peer pointing at peer_addr
    let directory = std::sync::Arc::new(PeerDirectory::new());
    let peer_info = std::sync::Arc::new(PeerInfo::new(peer_addr, PeerDirection::Outbound, peer_addr).with_ready(true));
    directory.insert(peer_info);
    // sanity-check directory contents
    let snap = directory.snapshot();
    assert_eq!(snap.len(), 1);
    assert!(snap[0].is_outbound_ready());

    // quick functional test: encode & send shards directly (bypass BroadcastWorker)
    {
        let sample_hash = kaspa_consensus_core::Hash::from_bytes([0xAA; 32]);
        let expected_packets = ShardGenerator::new(config, sample_hash, FtrBlock(data.clone())).count();
        let test_sender = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind test_sender");
        for shard in ShardGenerator::new(config, sample_hash, FtrBlock(data.clone())) {
            let wire = shard.to_bytes();
            let mac = auth.mac(&wire);
            let mut framed = Vec::with_capacity(mac.len() + wire.len());
            framed.extend_from_slice(&mac);
            framed.extend_from_slice(&wire);
            test_sender.send_to(&framed, peer_addr).expect("send shard");
        }
        // drain — log first few receipts for diagnosis
        let mut drained = 0usize;
        while drained < expected_packets {
            let mut buf = [0u8; 4096];
            match peer_recv.recv_from(&mut buf) {
                Ok(_) => {
                    drained += 1;
                }
                Err(_) => {
                    break;
                }
            }
        }
        assert_eq!(drained, expected_packets, "direct send test should deliver all shards");
    }

    // create control channels for verification workers (one sender)
    let mut control_txs = Vec::with_capacity(1);
    let (control_tx, _control_rx) = crossbeam_channel::bounded(transport.raw_channel_capacity);
    control_txs.push(control_tx);
    let control_txs = std::sync::Arc::new(control_txs);

    // spawn BroadcastWorker
    let (worker, broadcast_tx) = BroadcastWorker::new(
        worker_socket.clone(),
        directory.clone(),
        auth.clone(),
        config,
        local_ready.clone(),
        transport,
        control_txs,
    );
    let handle =
        std::thread::Builder::new().name("ftr-broadcast-bench".into()).spawn(move || worker.run()).expect("spawn broadcast worker");

    // give the worker thread a short moment to initialize (matches udp_receive_loop pattern)
    std::thread::sleep(Duration::from_millis(100));

    let sample_hash = kaspa_consensus_core::Hash::from_bytes([0xAA; 32]);
    let expected_packets = ShardGenerator::new(config, sample_hash, FtrBlock(data.clone())).index_of_last_generation();

    println!("BroadcastWorker bench: sending {} shards of size {} bytes", expected_packets, config.payload_size);

    // warm-up (best-effort) — give the worker a little time to finish the send
    broadcast_tx
        .try_send(kaspa_trusted_relay::broadcast::BroadcastMessage::BroadcastBlock {
            hash: sample_hash,
            block: FtrBlock(data.clone()),
        })
        .expect("warmup");
    std::thread::sleep(Duration::from_millis(50));
    let mut drained = 0usize;
    while drained < expected_packets {
        let mut buf = [0u8; 4096];
        match peer_recv.recv_from(&mut buf) {
            Ok((_n, _src)) => drained += 1,
            Err(_) => break,
        }
    }

    group.bench_function("broadcast_worker_1mb_end_to_end", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                let hash = iter_hash(0xBB, i);
                let expected =
                    ShardGenerator::new(config, black_box(hash), FtrBlock(black_box(data.clone()))).index_of_last_generation();
                peer_recv.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
                let start = Instant::now();
                broadcast_tx
                    .try_send(kaspa_trusted_relay::broadcast::BroadcastMessage::BroadcastBlock {
                        hash: black_box(hash),
                        block: FtrBlock(black_box(data.clone())),
                    })
                    .expect("send job");
                // wait for all shards to arrive at peer socket (log first few receipts)
                let mut received = 0usize;
                let deadline = Instant::now() + Duration::from_secs(10);
                while received < expected && Instant::now() < deadline {
                    let mut buf = black_box([0u8; 4096]);
                    match peer_recv.recv(&mut buf) {
                        Ok(_) => {
                            received += 1;
                        }
                        Err(_) => {
                            panic!(
                                "Timeout waiting for packets: iter{} received {}/{}: {:?}",
                                iters,
                                received,
                                expected,
                                std::io::Error::last_os_error()
                            );
                        }
                    }
                }
                if received != expected {
                    panic!("broadcast bench: expected {} packets, received {}", expected, received);
                }

                // record elapsed time for this iteration
                total += start.elapsed();
            }
            total
        })
    });

    // teardown
    drop(broadcast_tx);
    drop(peer_recv);
    handle.join().expect("broadcast worker join");

    group.finish();
}

/// Processing-only microbenchmark: verify MAC + parse Shard from bytes (no syscalls).
fn benchmark_recv_path_processing(c: &mut Criterion) {
    use kaspa_trusted_relay::auth::TokenAuthenticator;

    let mut group = c.benchmark_group("recv_path_processing");

    let frames_to_generate = 1024usize;
    let payload_size = 1200usize;
    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());

    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(frames_to_generate);
    for i in 0..frames_to_generate {
        let hash = kaspa_consensus_core::Hash::from_bytes([(i & 0xFF) as u8; 32]);
        let header = ShardHeader::new(hash, (i % 65535) as u16, 5);
        let shard = Shard::new(header, bytes::Bytes::from(vec![0xABu8; payload_size]));
        let wire = shard.to_bytes();
        let mac = auth.mac(&wire);
        let mut framed = Vec::with_capacity(mac.len() + wire.len());
        framed.extend_from_slice(&mac);
        framed.extend_from_slice(&wire);
        frames.push(framed);
    }

    group.bench_function("process_packets", |b| {
        b.iter(|| {
            for frame in frames.iter().take(256) {
                let mac = &frame[..kaspa_trusted_relay::auth::AuthToken::TOKEN_SIZE];
                let data = &frame[kaspa_trusted_relay::auth::AuthToken::TOKEN_SIZE..];

                // Verify MAC
                debug_assert!(auth.verify_mac(data, mac));

                // Parse shard (zero-copy into `Bytes` then `Shard::from`)
                let bytes = bytes::Bytes::copy_from_slice(data);
                let shard = Shard::from(bytes);
                black_box(shard.header().shard_index());
            }
        });
    });

    group.finish();
}

// =============================================================================
// FULL-PIPELINE BENCHMARK
// =============================================================================
//
// Exercises the complete ingest path:
//   pre-framed UDP shards → UdpReceiveLoop (collector → verifier workers)
//   → Coordinator (FEC decode) → decoded block received.
//
// The block is sharded and MAC-framed *outside* the timed region so we
// measure only the runtime cost of: UDP recv → MAC verify → dedup →
// channel hop → coordinator reassembly → FEC decode → block delivery.
//
// Parameterised by `(hmac_workers, decode_workers)` so we can sweep thread
// configurations in a single bench run.
fn benchmark_full_pipeline(c: &mut Criterion) {
    use arc_swap::ArcSwap;
    use kaspa_trusted_relay::auth::TokenAuthenticator;
    use kaspa_trusted_relay::params::{DecodingParams, TransportParams};
    use kaspa_trusted_relay::servers::udp_transport::ForwardJob;
    use kaspa_trusted_relay::servers::udp_transport::UdpReceiveLoop;
    use kaspa_trusted_relay::servers::udp_transport::VerificationMessage;
    use kaspa_trusted_relay::fragmentation::decoding::block_reassembler::Coordinator;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    let mut group = c.benchmark_group("full_pipeline");
    // Each iteration sends hundreds of packets — keep sample size modest.
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(15));

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let sharding_config = ShardingConfig::new(16, 4, 1200);

    const BLOCK_SIZE: usize = 500 * 1024; // 0.5 MB

    // Thread configurations to sweep: (collectors, hmac_workers, decode_workers, coordinators)
    let configs: Vec<(&str, usize, usize, usize, usize)> = vec![
        ("1c_1v_1d_1c", 1, 1, 1, 1),
        ("1c_2v_2d_1c", 1, 2, 2, 1),
        ("2c_4v_2d_1c", 2, 4, 2, 1),
        ("2c_4v_4d_2c", 2, 4, 4, 2),
        ("4c_8v_4d_2c", 4, 8, 4, 2),
    ];

    // Iteration counter for unique hashes
    static PIPELINE_ITER: AtomicU64 = AtomicU64::new(0);

    for (label, num_collectors, hmac_workers, decode_workers, coordinators) in configs {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(num_collectors, hmac_workers, decode_workers, coordinators),
            |b, &(num_cols, hmac_w, dec_w, coord_n)| {
                // ---- infrastructure (created once per parameter set) ----
                //
                // We keep infrastructure alive across iterations to avoid the ~100ms
                // thread-spawn + sleep overhead that would otherwise dominate every
                // measurement.  Memory is kept bounded by draining any stale decoded
                // blocks from the output channel between iterations.

                // Recv sockets (one per collector with SO_REUSEPORT)
                let base_sock2 = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))
                    .expect("create base socket2");
                let _ = base_sock2.set_recv_buffer_size(16 * 1024 * 1024);
                let _ = base_sock2.set_send_buffer_size(8 * 1024 * 1024);
                let _ = base_sock2.set_reuse_address(true);
                let _ = base_sock2.set_reuse_port(true);
                let _ = base_sock2.set_nonblocking(false);
                base_sock2.bind(&"127.0.0.1:0".parse::<std::net::SocketAddr>().expect("parse addr").into()).expect("bind recv");
                let base_socket = std::net::UdpSocket::from(base_sock2);
                let recv_addr = base_socket.local_addr().expect("recv addr");

                let mut collector_sockets = vec![Arc::new(base_socket)];
                for _ in 1..num_cols {
                    let sock2 = socket2::Socket::new(
                        socket2::Domain::for_address(recv_addr),
                        socket2::Type::DGRAM,
                        Some(socket2::Protocol::UDP),
                    )
                    .expect("create socket2");
                    let _ = sock2.set_recv_buffer_size(16 * 1024 * 1024);
                    let _ = sock2.set_send_buffer_size(8 * 1024 * 1024);
                    let _ = sock2.set_reuse_address(true);
                    let _ = sock2.set_reuse_port(true);
                    let _ = sock2.set_nonblocking(false);
                    sock2.bind(&recv_addr.into()).expect("bind collector");
                    collector_sockets.push(Arc::new(std::net::UdpSocket::from(sock2)));
                }

                // Coordinators
                let decoding = DecodingParams { num_workers: dec_w, block_reassembly_workers: coord_n, ..DecodingParams::default() };
                let mut shard_txs = Vec::new();
                let mut block_rxs = Vec::new();
                for i in 0..coord_n {
                    let (coordinator, shard_tx, block_rx) = Coordinator::new(sharding_config, decoding).unwrap();
                    shard_txs.push(shard_tx);
                    block_rxs.push(block_rx);
                    std::thread::Builder::new()
                        .name(format!("bench-coord-{}", i))
                        .spawn(move || coordinator.run())
                        .expect("spawn coordinator");
                }
                let shard_txs = Arc::new(shard_txs);

                // Merge block_rxs into a single receiver
                let (block_rx, _merge_handle): (crossbeam_channel::Receiver<(Hash, FtrBlock)>, _) = {
                    let (merged_tx, merged_rx) = crossbeam_channel::bounded::<(Hash, FtrBlock)>(256);
                    let rxs = block_rxs;
                    let h = std::thread::Builder::new()
                        .name("bench-merge".into())
                        .spawn(move || {
                            use crossbeam_channel::Select;
                            let mut sel = Select::new();
                            for rx in &rxs {
                                sel.recv(rx);
                            }
                            loop {
                                let oper = sel.select();
                                let idx = oper.index();
                                match oper.recv(&rxs[idx]) {
                                    Ok(item) => {
                                        if merged_tx.send(item).is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        })
                        .expect("spawn merge");
                    (merged_rx, h)
                };

                // Transport params
                let transport = TransportParams {
                    hmac_workers: hmac_w,
                    num_collectors: num_cols,
                    default_buffer_size: 4096,
                    ..TransportParams::default()
                };

                let (forwarder_tx, _forwarder_rx) = crossbeam_channel::bounded::<ForwardJob>(transport.forward_channel_capacity);

                let mut control_txs = Vec::with_capacity(hmac_w);
                for _ in 0..hmac_w {
                    let (tx, _rx) = crossbeam_channel::bounded::<VerificationMessage>(transport.raw_channel_capacity);
                    control_txs.push(tx);
                }
                let control_txs = Arc::new(control_txs);

                let allowlist = Arc::new(ArcSwap::from_pointee(HashSet::new()));
                let is_ready = Arc::new(std::sync::atomic::AtomicBool::new(true));

                let recv_loop = UdpReceiveLoop::with_sockets(
                    collector_sockets,
                    shard_txs.clone(),
                    allowlist.clone(),
                    auth.clone(),
                    forwarder_tx,
                    is_ready,
                    sharding_config,
                    transport,
                    control_txs,
                );
                let udp_handle = recv_loop.run();

                std::thread::sleep(Duration::from_millis(100));
                let send_socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind send");
                set_recv_buf(&send_socket, 16 * 1024 * 1024);
                let sender_addr = send_socket.local_addr().expect("sender addr");
                allowlist.rcu(|old| {
                    let mut s = (**old).clone();
                    s.insert(sender_addr);
                    s
                });

                // ---- timed benchmark ----
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for i in 0..iters {
                        let iter_id = PIPELINE_ITER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let hash = iter_hash(0xFF, iter_id);

                        // Pre-encode block into framed UDP packets (outside measurement)
                        let test_data = vec![0x42u8; BLOCK_SIZE];
                        let shards: Vec<Shard> = ShardGenerator::new(sharding_config, hash, FtrBlock(test_data)).collect();
                        let mut frames: Vec<Vec<u8>> = Vec::with_capacity(shards.len());
                        for shard in &shards {
                            let wire = shard.to_bytes();
                            let mac = auth.mac(&wire);
                            let mut framed = Vec::with_capacity(mac.len() + wire.len());
                            framed.extend_from_slice(&mac);
                            framed.extend_from_slice(&wire);
                            frames.push(framed);
                        }

                        // ---- TIMED: send → verify → decode → receive ----
                        let start = Instant::now();

                        for frame in &frames {
                            let _ = send_socket.send_to(frame, recv_addr);
                        }

                        match block_rx.recv_timeout(Duration::from_secs(10)) {
                            Ok((_h, data)) => {
                                black_box(&data);
                            }
                            Err(e) => {
                                panic!("full_pipeline: timeout waiting for decoded block (iter {}, config {}): {:?}", i, label, e);
                            }
                        }

                        total += start.elapsed();

                        // Drain any extra decoded blocks to prevent memory accumulation.
                        // This keeps the output channel empty between iterations so decoded
                        // FtrBlock data (each ~500KB) doesn't pile up across hundreds of runs.
                        while block_rx.try_recv().is_ok() {}
                    }

                    total
                });

                // ---- Teardown ----
                // Drop UdpReceiveHandle → calls socket.shutdown(Shutdown::Both) on
                // every collector socket → instantly unblocks recv_from → collectors exit
                // → workers exit → coordinators exit → merge exits.
                drop(udp_handle);
                drop(shard_txs);
                drop(block_rx);
                drop(send_socket);
                // Brief pause to let threads finish unwinding.
                std::thread::sleep(Duration::from_millis(50));
            },
        );
    }

    group.finish();
}

// =============================================================================
// THROUGHPUT BENCHMARK  —  multiple blocks, lossy vs. lossless
// =============================================================================
//
// Fires `NUM_BLOCKS` blocks through the full ingest pipeline and measures the
// wall-clock time until **all** decoded blocks have been received.
//
// Two loss modes:
//   • **lossless** — every shard is sent (tests fast-path / data-only decode).
//   • **lossy**    — one random *data* shard per generation is dropped, forcing
//                    Reed-Solomon recovery for every generation.
//
// Parameterised by `(hmac_workers, decode_workers, coordinators)` so we can
// sweep thread configurations in a single bench run.
fn benchmark_throughput(c: &mut Criterion) {
    use arc_swap::ArcSwap;
    use kaspa_trusted_relay::auth::TokenAuthenticator;
    use kaspa_trusted_relay::params::{DecodingParams, TransportParams};
    use kaspa_trusted_relay::servers::udp_transport::ForwardJob;
    use kaspa_trusted_relay::servers::udp_transport::UdpReceiveLoop;
    use kaspa_trusted_relay::servers::udp_transport::VerificationMessage;
    use kaspa_trusted_relay::fragmentation::decoding::block_reassembler::Coordinator;
    use rand::Rng as _;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    let mut group = c.benchmark_group("throughput");
    // We're sending many blocks — keep iterations low.
    group.sample_size(15);
    group.measurement_time(Duration::from_secs(20));

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let sharding_config = ShardingConfig::new(16, 4, 1200);

    const BLOCK_SIZE: usize = 500 * 1024; // 0.5 MB
    const NUM_BLOCKS: usize = 20;

    // Thread configurations to sweep: (num_collectors, hmac_workers, decode_workers, coordinators)
    let thread_configs: Vec<(&str, usize, usize, usize, usize)> =
        vec![("1c_1v_1d_1c", 1, 1, 1, 1), ("2c_2v_2d_1c", 2, 2, 2, 1), ("2c_1v_2d_1c", 2, 1, 2, 1), ("4c_1v_4d_2c", 4, 1, 4, 2)];

    let loss_modes: Vec<(&str, bool)> = vec![("lossless", false), ("lossy", true)];

    for (tc_label, num_collectors, hmac_workers, decode_workers, coordinators) in &thread_configs {
        for (loss_label, lossy) in &loss_modes {
            let bench_id = format!("{}_{}_{tc_label}", NUM_BLOCKS, loss_label);

            group.bench_function(BenchmarkId::from_parameter(&bench_id), |b| {
                // Iteration counter
                static THROUGHPUT_ITER: AtomicU64 = AtomicU64::new(0);

                // ---- infrastructure (created once per parameter set) ----
                //
                // We keep infrastructure alive across iterations to avoid the ~100ms
                // thread-spawn + sleep overhead that would otherwise dominate every
                // measurement.  Memory is kept bounded by draining any stale decoded
                // blocks from the output channel between iterations.

                // Recv socket
                let base_sock2 = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))
                    .expect("create base socket2");
                let _ = base_sock2.set_recv_buffer_size(32 * 1024 * 1024);
                let _ = base_sock2.set_send_buffer_size(8 * 1024 * 1024);
                let _ = base_sock2.set_reuse_address(true);
                let _ = base_sock2.set_reuse_port(true);
                let _ = base_sock2.set_nonblocking(false);
                base_sock2.bind(&"127.0.0.1:0".parse::<std::net::SocketAddr>().expect("parse addr").into()).expect("bind recv");
                let base_socket = std::net::UdpSocket::from(base_sock2);
                let recv_addr = base_socket.local_addr().expect("recv addr");
                let collector_sockets = vec![Arc::new(base_socket)];

                // Coordinators — bump input queue for multi-block throughput
                let decoding = DecodingParams {
                    num_workers: *decode_workers,
                    block_reassembly_workers: *coordinators,
                    input_queue_capacity: NUM_BLOCKS * 600,
                    ..DecodingParams::default()
                };
                let mut shard_txs = Vec::new();
                let mut block_rxs = Vec::new();
                for i in 0..*coordinators {
                    let (coordinator, shard_tx, block_rx) = Coordinator::new(sharding_config, decoding).unwrap();
                    shard_txs.push(shard_tx);
                    block_rxs.push(block_rx);
                    std::thread::Builder::new()
                        .name(format!("bench-coord-{}", i))
                        .spawn(move || coordinator.run())
                        .expect("spawn coordinator");
                }
                let shard_txs = Arc::new(shard_txs);

                // Merge block_rxs into a single receiver
                let (block_rx, _merge_handle): (crossbeam_channel::Receiver<(Hash, FtrBlock)>, _) = {
                    let (merged_tx, merged_rx) = crossbeam_channel::bounded::<(Hash, FtrBlock)>(256);
                    let rxs = block_rxs;
                    let h = std::thread::Builder::new()
                        .name("bench-merge".into())
                        .spawn(move || {
                            use crossbeam_channel::Select;
                            let mut sel = Select::new();
                            for rx in &rxs {
                                sel.recv(rx);
                            }
                            loop {
                                let oper = sel.select();
                                let idx = oper.index();
                                match oper.recv(&rxs[idx]) {
                                    Ok(item) => {
                                        if merged_tx.send(item).is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        })
                        .expect("spawn merge");
                    (merged_rx, h)
                };

                // Transport params — bump raw_channel_capacity for multi-block burst
                let transport = TransportParams {
                    hmac_workers: *hmac_workers,
                    num_collectors: *num_collectors,
                    default_buffer_size: 4096,
                    raw_channel_capacity: NUM_BLOCKS * 600,
                    ..TransportParams::default()
                };

                let (forwarder_tx, _forwarder_rx) = crossbeam_channel::bounded::<ForwardJob>(transport.forward_channel_capacity);

                let mut control_txs = Vec::with_capacity(*hmac_workers);
                for _ in 0..*hmac_workers {
                    let (tx, _rx) = crossbeam_channel::bounded::<VerificationMessage>(transport.raw_channel_capacity);
                    control_txs.push(tx);
                }
                let control_txs = Arc::new(control_txs);

                let allowlist = Arc::new(ArcSwap::from_pointee(HashSet::new()));
                let is_ready = Arc::new(std::sync::atomic::AtomicBool::new(true));

                let recv_loop = UdpReceiveLoop::with_sockets(
                    collector_sockets,
                    shard_txs.clone(),
                    allowlist.clone(),
                    auth.clone(),
                    forwarder_tx,
                    is_ready,
                    sharding_config,
                    transport,
                    control_txs,
                );
                let udp_handle = recv_loop.run();

                std::thread::sleep(Duration::from_millis(100));
                let send_socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind send");
                set_recv_buf(&send_socket, 32 * 1024 * 1024);
                let sender_addr = send_socket.local_addr().expect("sender addr");
                allowlist.rcu(|old| {
                    let mut s = (**old).clone();
                    s.insert(sender_addr);
                    s
                });

                let is_lossy = *lossy;

                // ---- timed benchmark ----
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _iter in 0..iters {
                        let base = THROUGHPUT_ITER.fetch_add(NUM_BLOCKS as u64, std::sync::atomic::Ordering::Relaxed);
                        let mut rng = rand::thread_rng();
                        let gen_size = sharding_config.shards_per_generation();

                        // Pre-encode all blocks outside timed section
                        let mut all_frames: Vec<Vec<Vec<u8>>> = Vec::with_capacity(NUM_BLOCKS);
                        for blk in 0..NUM_BLOCKS {
                            let hash = iter_hash(0xEE, base + blk as u64);
                            let test_data = vec![0x42u8; BLOCK_SIZE];
                            let shards: Vec<Shard> = ShardGenerator::new(sharding_config, hash, FtrBlock(test_data)).collect();

                            let mut frames: Vec<Vec<u8>> = Vec::with_capacity(shards.len());

                            let num_gens = shards.len().div_ceil(gen_size);
                            let drop_per_gen: Vec<usize> = if is_lossy {
                                (0..num_gens)
                                    .map(|g| {
                                        let k = if g == num_gens - 1 {
                                            let rem = shards.len() % gen_size;
                                            if rem == 0 { sharding_config.data_blocks } else { rem.min(sharding_config.data_blocks) }
                                        } else {
                                            sharding_config.data_blocks
                                        };
                                        rng.gen_range(0..k)
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            for (idx, shard) in shards.iter().enumerate() {
                                if is_lossy {
                                    let g = idx / gen_size;
                                    let idx_in_gen = idx % gen_size;
                                    if idx_in_gen == drop_per_gen[g] {
                                        continue;
                                    }
                                }

                                let wire = shard.to_bytes();
                                let mac = auth.mac(&wire);
                                let mut framed = Vec::with_capacity(mac.len() + wire.len());
                                framed.extend_from_slice(&mac);
                                framed.extend_from_slice(&wire);
                                frames.push(framed);
                            }
                            all_frames.push(frames);
                        }

                        // ---- TIMED: send all blocks → receive all decoded ----
                        let start = Instant::now();

                        for frames in &all_frames {
                            for frame in frames {
                                let _ = send_socket.send_to(frame, recv_addr);
                            }
                        }

                        let mut blocks_received = 0usize;
                        while blocks_received < NUM_BLOCKS {
                            match block_rx.recv_timeout(Duration::from_secs(30)) {
                                Ok((_h, data)) => {
                                    black_box(&data);
                                    blocks_received += 1;
                                }
                                Err(e) => {
                                    panic!(
                                        "throughput: timeout after {}/{} blocks \
                                         (config {}, {}): {:?}",
                                        blocks_received, NUM_BLOCKS, tc_label, loss_label, e
                                    );
                                }
                            }
                        }

                        total += start.elapsed();

                        // Drain any extra decoded blocks to prevent memory accumulation.
                        while block_rx.try_recv().is_ok() {}
                    }

                    total
                });

                // ---- Teardown ----
                // Drop UdpReceiveHandle → socket.shutdown() → instant unblock.
                drop(udp_handle);
                drop(shard_txs);
                drop(block_rx);
                drop(send_socket);
                std::thread::sleep(Duration::from_millis(50));
            });
        }
    }

    group.finish();
}

// =============================================================================
// CONGESTION BENCHMARK  —  N peers sending the *same* block
// =============================================================================
//
// Simulates multiple relay peers that all broadcast the same block (same hash,
// same shards) simultaneously.  Only **one** decoded block should emerge.
//
// The verifiers' per-worker `RingMap<Hash, FixedBitSet>` dedup ensures that
// only the first copy of each shard goes through MAC verification, and all
// subsequent copies are rejected with a cheap bitset check *before* the
// expensive HMAC.
//
// The benchmark sweeps `num_peers × hmac_workers` to show whether extra
// verifier threads help absorb dedup load under congestion.
//
// Infrastructure is created once per parameter set (outside iter_custom).
// Pre-spawned sender threads wait on a `Barrier` for synchronized blasts.
fn benchmark_congestion(c: &mut Criterion) {
    use arc_swap::ArcSwap;
    use kaspa_trusted_relay::auth::TokenAuthenticator;
    use kaspa_trusted_relay::params::{DecodingParams, TransportParams};
    use kaspa_trusted_relay::servers::udp_transport::ForwardJob;
    use kaspa_trusted_relay::servers::udp_transport::UdpReceiveLoop;
    use kaspa_trusted_relay::servers::udp_transport::VerificationMessage;
    use kaspa_trusted_relay::fragmentation::decoding::block_reassembler::Coordinator;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Barrier};

    let mut group = c.benchmark_group("congestion");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(15));

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let sharding_config = ShardingConfig::new(16, 4, 1200);

    const BLOCK_SIZE: usize = 500 * 1024; // 0.5 MB

    // (label, num_collectors, num_peers, hmac_workers, decode_workers, coordinators)
    let configs: Vec<(&str, usize, usize, usize, usize, usize)> = vec![
        // We are generous with the coordinator and decoder sharding, to ensure these are not bottlenecks.
        // Congestion with same verifiers and collectors
        ("1c_12p_1v", 1, 12, 1, 8, 1),
        ("2c_12p_2v", 2, 12, 2, 8, 1),
        ("4c_12p_4v", 4, 12, 4, 8, 1),
        // Congestion with fewer verifiers
        ("2c_12p_1v", 2, 12, 1, 8, 1),
        ("4c_12p_2v", 4, 12, 2, 8, 1),
        ("4c_12p_1v", 4, 12, 1, 8, 1),
        // Congestion with few collectors
        ("1c_12p_2v", 1, 12, 2, 8, 1),
        ("2c_12p_4v", 2, 12, 4, 8, 1),
        ("1c_12p_4v", 1, 12, 4, 8, 1),
    ];

    // Iteration counter for unique hashes
    static CONGESTION_ITER: AtomicU64 = AtomicU64::new(0);

    for (label, num_collectors, num_peers, hmac_workers, decode_workers, coordinators) in configs {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(num_collectors, num_peers, hmac_workers, decode_workers, coordinators),
            |b, &(n_cols, n_peers, hmac_w, dec_w, coord_n)| {
                // ---- infrastructure (created once per parameter set) ----
                //
                // We keep infrastructure alive across iterations to avoid the ~100ms
                // thread-spawn + sleep overhead that would otherwise dominate every
                // measurement.  Memory is kept bounded by draining any stale decoded
                // blocks from the output channel between iterations.

                // Recv sockets (one per collector with SO_REUSEPORT)
                let base_sock2 = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))
                    .expect("create base socket2");
                let _ = base_sock2.set_recv_buffer_size(32 * 1024 * 1024);
                let _ = base_sock2.set_send_buffer_size(8 * 1024 * 1024);
                let _ = base_sock2.set_reuse_address(true);
                let _ = base_sock2.set_reuse_port(true);
                let _ = base_sock2.set_nonblocking(false);
                base_sock2.bind(&"127.0.0.1:0".parse::<std::net::SocketAddr>().expect("parse addr").into()).expect("bind recv");
                let base_socket = std::net::UdpSocket::from(base_sock2);
                let recv_addr = base_socket.local_addr().expect("recv addr");

                let mut collector_sockets = vec![Arc::new(base_socket)];
                for _ in 1..n_cols {
                    let sock2 = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))
                        .expect("create collector socket2");
                    let _ = sock2.set_recv_buffer_size(32 * 1024 * 1024);
                    let _ = sock2.set_send_buffer_size(8 * 1024 * 1024);
                    let _ = sock2.set_reuse_address(true);
                    let _ = sock2.set_reuse_port(true);
                    let _ = sock2.set_nonblocking(false);
                    sock2.bind(&recv_addr.into()).expect("bind collector");
                    collector_sockets.push(Arc::new(std::net::UdpSocket::from(sock2)));
                }

                // Coordinator(s)
                let decoding = DecodingParams {
                    num_workers: dec_w,
                    block_reassembly_workers: coord_n,
                    input_queue_capacity: 2048,
                    ..DecodingParams::default()
                };
                let mut shard_txs = Vec::new();
                let mut block_rxs = Vec::new();
                for i in 0..coord_n {
                    let (coordinator, shard_tx, block_rx) = Coordinator::new(sharding_config, decoding).unwrap();
                    shard_txs.push(shard_tx);
                    block_rxs.push(block_rx);
                    std::thread::Builder::new()
                        .name(format!("cong-coord-{}", i))
                        .spawn(move || coordinator.run())
                        .expect("spawn coordinator");
                }
                let shard_txs = Arc::new(shard_txs);

                // Merge block_rxs
                let (block_rx, _merge_handle): (crossbeam_channel::Receiver<(Hash, FtrBlock)>, _) = {
                    let (merged_tx, merged_rx) = crossbeam_channel::bounded::<(Hash, FtrBlock)>(256);
                    let rxs = block_rxs;
                    let h = std::thread::Builder::new()
                        .name("cong-merge".into())
                        .spawn(move || {
                            use crossbeam_channel::Select;
                            let mut sel = Select::new();
                            for rx in &rxs {
                                sel.recv(rx);
                            }
                            loop {
                                let oper = sel.select();
                                let idx = oper.index();
                                match oper.recv(&rxs[idx]) {
                                    Ok(item) => {
                                        if merged_tx.send(item).is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        })
                        .expect("spawn merge");
                    (merged_rx, h)
                };

                // Transport
                let transport = TransportParams {
                    hmac_workers: hmac_w,
                    num_collectors: n_cols,
                    default_buffer_size: 4096,
                    ..TransportParams::default()
                };

                let (forwarder_tx, _forwarder_rx) = crossbeam_channel::bounded::<ForwardJob>(transport.forward_channel_capacity);

                let mut control_txs = Vec::with_capacity(hmac_w);
                for _ in 0..hmac_w {
                    let (tx, _rx) = crossbeam_channel::bounded::<VerificationMessage>(transport.raw_channel_capacity);
                    control_txs.push(tx);
                }
                let control_txs = Arc::new(control_txs);

                let allowlist = Arc::new(ArcSwap::from_pointee(HashSet::new()));
                let is_ready = Arc::new(std::sync::atomic::AtomicBool::new(true));

                let recv_loop = UdpReceiveLoop::with_sockets(
                    collector_sockets,
                    shard_txs.clone(),
                    allowlist.clone(),
                    auth.clone(),
                    forwarder_tx,
                    is_ready,
                    sharding_config,
                    transport,
                    control_txs,
                );
                let udp_handle = recv_loop.run();
                std::thread::sleep(Duration::from_millis(100));

                // Create N peer sender sockets, all allowlisted
                let peer_sockets: Vec<std::net::UdpSocket> = (0..n_peers)
                    .map(|_| {
                        let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind peer");
                        set_recv_buf(&sock, 8 * 1024 * 1024);
                        let addr = sock.local_addr().expect("peer addr");
                        allowlist.rcu(|old| {
                            let mut s = (**old).clone();
                            s.insert(addr);
                            s
                        });
                        sock
                    })
                    .collect();

                // Pre-spawn sender threads with channels for work dispatch
                let (work_txs, done_rx) = {
                    let mut work_txs = Vec::with_capacity(n_peers);
                    let (done_tx, done_rx) = crossbeam_channel::bounded::<()>(n_peers);

                    for (peer_idx, sock) in peer_sockets.into_iter().enumerate() {
                        let (work_tx, work_rx) = crossbeam_channel::bounded::<(Arc<Vec<Vec<u8>>>, Arc<Barrier>)>(1);
                        let done_tx = done_tx.clone();
                        std::thread::Builder::new()
                            .name(format!("cong-peer-{}", peer_idx))
                            .spawn(move || {
                                while let Ok((frames, barrier)) = work_rx.recv() {
                                    barrier.wait();
                                    for frame in frames.iter() {
                                        let _ = sock.send_to(frame, recv_addr);
                                    }
                                    let _ = done_tx.send(());
                                }
                            })
                            .expect("spawn peer sender");
                        work_txs.push(work_tx);
                    }
                    (work_txs, done_rx)
                };

                // ---- timed benchmark ----
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _ in 0..iters {
                        let iter_id = CONGESTION_ITER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let hash = iter_hash(0xCC, iter_id);

                        // Pre-encode one block (outside measurement)
                        let test_data = vec![0x42u8; BLOCK_SIZE];
                        let shards: Vec<Shard> = ShardGenerator::new(sharding_config, hash, FtrBlock(test_data)).collect();
                        let mut frames: Vec<Vec<u8>> = Vec::with_capacity(shards.len());
                        for shard in &shards {
                            let wire = shard.to_bytes();
                            let mac = auth.mac(&wire);
                            let mut framed = Vec::with_capacity(mac.len() + wire.len());
                            framed.extend_from_slice(&mac);
                            framed.extend_from_slice(&wire);
                            frames.push(framed);
                        }

                        let frames = Arc::new(frames);
                        let barrier = Arc::new(Barrier::new(n_peers + 1));

                        // Dispatch frames to all peer threads
                        for tx in &work_txs {
                            tx.send((frames.clone(), barrier.clone())).expect("send work");
                        }

                        // ---- TIMED: release barrier → receive decoded block ----
                        let start = Instant::now();
                        barrier.wait();

                        // Expect exactly 1 decoded block
                        match block_rx.recv_timeout(Duration::from_secs(10)) {
                            Ok((_h, data)) => {
                                black_box(&data);
                            }
                            Err(e) => {
                                panic!(
                                    "congestion: timeout waiting for decoded \
                                     block (config {}, {} peers): {:?}",
                                    label, n_peers, e
                                );
                            }
                        }

                        total += start.elapsed();

                        // Wait for all senders to finish (they may still be
                        // blasting duplicates after the block was decoded)
                        for _ in 0..n_peers {
                            let _ = done_rx.recv_timeout(Duration::from_secs(5));
                        }

                        // Drain any extra decoded blocks to prevent memory accumulation.
                        while block_rx.try_recv().is_ok() {}
                    }

                    total
                });

                // ---- Teardown ----
                // Drop UdpReceiveHandle → socket.shutdown() → instant unblock.
                drop(work_txs);
                drop(udp_handle);
                drop(shard_txs);
                drop(block_rx);
                std::thread::sleep(Duration::from_millis(50));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    //benchmark_shard_generation,
    //benchmark_shard_generation_small,
    //benchmark_different_configs,
    //benchmark_decoding_perfect_reception,
    //benchmark_decoding_fast_path,
    //benchmark_decoding_with_loss,
    //benchmark_decoding_different_k_m,
    //benchmark_shard_serialization,
    //benchmark_mac_algorithms,
    benchmark_udp_receive_loop,
    benchmark_broadcast_worker_e2e_1mb,
    //benchmark_recv_path_processing,
    //benchmark_full_pipeline,
    benchmark_throughput,
    benchmark_congestion,
);
criterion_main!(benches);
