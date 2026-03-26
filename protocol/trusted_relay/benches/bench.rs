use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce, aead::Aead};
use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use hmac::{Hmac, Mac};

use poly1305::universal_hash::UniversalHash;
use rand::random;
use sha2::Sha256;
use tokio::sync::Mutex as TokioMutex;

type HmacSha256 = Hmac<Sha256>;

use kaspa_consensus_core::Hash;
use kaspa_trusted_relay::codec::encoder::FragmentGenerator;
use kaspa_trusted_relay::model::fragments::{Fragment, FragmentHeader};
use kaspa_trusted_relay::model::ftr_block::FtrBlock;
use kaspa_trusted_relay::params::{FragmentationConfig, TransportParams};
use kaspa_trusted_relay::servers::auth::{AuthToken, TokenAuthenticator};
use kaspa_trusted_relay::servers::peer_directory::{PeerDirectory, PeerInfo};
use kaspa_trusted_relay::servers::tcp_control::PeerDirection;
use kaspa_trusted_relay::servers::udp_transport::pipeline::broadcast::{BroadcastMessage, spawn_broadcaster_thread};
use kaspa_trusted_relay::servers::udp_transport::pipeline::reassembly::reassembler::BlockReassemblerBlockMessage;
use kaspa_trusted_relay::servers::udp_transport::pipeline::verification::VerificationMessage;
use kaspa_trusted_relay::servers::udp_transport::runtime::TransportRuntime;

/// Enlarge the kernel UDP receive buffer on a std socket.
/// Uses raw setsockopt on Unix; no-op on other platforms.
#[cfg(unix)]
fn set_recv_buf(sock: &std::net::UdpSocket, size: usize) -> std::net::UdpSocket {
    use socket2::Socket;

    let sock = Socket::from(sock.try_clone().expect("Failed to clone socket"));
    // Best-effort buffer sizing - OS may cap to system limits (e.g. /proc/sys/net/core/rmem_max)
    let _ = sock.set_recv_buffer_size(size);
    let _ = sock.set_send_buffer_size(size);
    sock.set_nonblocking(false).expect("Failed to set non-blocking mode");
    //sock.set_nodelay(true).expect("Failed to set TCP_NODELAY");
    sock.set_reuse_address(true).expect("Failed to set SO_REUSEADDR");
    sock.set_reuse_port(true).expect("Failed to set SO_REUSEPORT");
    assert!(sock.recv_buffer_size().expect("Failed to get SO_RCVBUF") >= size);
    assert!(sock.send_buffer_size().expect("Failed to get SO_SNDBUF") >= size);
    sock.into()
}

#[cfg(not(unix))]
fn set_recv_buf(_sock: &std::net::UdpSocket, _size: usize) -> std::net::UdpSocket {
    // No-op on unsupported platforms.
    unimplemented!("set_recv_buf is only implemented on Unix platforms");
}

/// Build a unique hash for each benchmark iteration so the pipeline's
/// processed-block cache never rejects a repeat.
#[inline]
fn random_hash() -> Hash {
    Hash::from_bytes(random())
}

/// Build MAC-framed UDP packets from fragments.
fn frame_fragments(fragments: &[Fragment], auth: &TokenAuthenticator) -> Vec<Vec<u8>> {
    let mut frames = Vec::with_capacity(fragments.len());
    for fragment in fragments {
        let wire = fragment.to_bytes();
        let mac = auth.mac(&wire);
        let mut framed = Vec::with_capacity(mac.len() + wire.len());
        framed.extend_from_slice(&mac);
        framed.extend_from_slice(&wire);
        frames.push(framed);
    }
    frames
}

/// Helper: set up a TransportRuntime for benchmarking.
/// Returns `(runtime, block_rx, tokio_rt)`.
#[allow(clippy::type_complexity)]
fn make_loopback_pipeline(
    config: FragmentationConfig,
    auth: &TokenAuthenticator,
    transport: TransportParams,
) -> (
    std::net::UdpSocket,
    SocketAddr,
    TransportRuntime,
    Arc<TokioMutex<tokio::sync::mpsc::UnboundedReceiver<BlockReassemblerBlockMessage>>>,
    tokio::runtime::Runtime,
    Arc<PeerDirectory>,
) {
    // Create sender socket BEFORE starting runtime so we can add it to allowlist.
    let send_socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("Failed to bind sender socket");
    let send_socket = set_recv_buf(&send_socket, 1024 * 1024 * 32);
    let sender_addr = send_socket.local_addr().expect("Failed to get sender socket address");

    // NOTE: PeerDirectory::insert_peer() does NOT update the allowlist ArcSwap —
    // the allowlist is only seeded from the HashMap passed to PeerDirectory::new().
    // The verifier checks this allowlist, so the sender MUST be in the initial map.
    let mut allowlist_map = HashMap::new();
    allowlist_map.insert(sender_addr.ip(), PeerDirection::Both);
    let directory = Arc::new(PeerDirectory::new(allowlist_map));
    let authenticator = Arc::new(auth.clone());

    let listen_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let mut runtime = TransportRuntime::new(transport, listen_addr, config, directory.clone(), authenticator.clone());
    runtime.start();

    let block_rx = runtime.block_receive();
    let tokio_rt = tokio::runtime::Runtime::new().unwrap();

    // Brief pause for worker threads to initialize and bind socket.
    std::thread::sleep(Duration::from_millis(10000));

    // Get the actual bound address where the runtime is listening.
    let recv_addr = runtime.local_addr().expect("TransportRuntime must expose bound address after start()");

    (send_socket, recv_addr, runtime, block_rx, tokio_rt, directory)
}

// ============================================================================
// ENCODING BENCHMARKS — FragmentGenerator throughput
// ============================================================================

fn benchmark_fragment_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_generation");

    // Test different data sizes
    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("500KB", 500 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            b.iter(|| {
                let test_data = black_box(vec![42u8; size]);
                let config = FragmentationConfig::new(4, 2, 1200);
                let hash = Hash::from([1u8; 32]);

                let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
                let _fragments: Vec<_> = black_box(generator.collect());
            });
        });
    }

    group.finish();
}

fn benchmark_fragment_generation_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_generation_small");

    // Single generation benchmarks
    let config = FragmentationConfig::new(4, 2, 1200);
    let single_gen_size = 4 * 1200; // One full generation

    group.bench_function("single_generation", |b| {
        b.iter(|| {
            let test_data = black_box(vec![42u8; single_gen_size]);
            let hash = Hash::from([2u8; 32]);

            let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
            let _fragments: Vec<_> = black_box(generator.collect());
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
                let config = FragmentationConfig::new(k, m, 1200);
                let hash = Hash::from([3u8; 32]);

                let generator = FragmentGenerator::new(config, hash, FtrBlock(black_box(test_data.clone())));
                let _fragments: Vec<_> = black_box(generator.collect());
            });
        });
    }

    group.finish();
}

// ============================================================================
// DECODING BENCHMARKS — measures only fragment send + pipeline decode + receive.
//
// The old `Coordinator` type was removed during the refactor.  These benchmarks
// now use the public `TransportRuntime` to stand up the full ingest pipeline
// (collector → verifier → reassembler → decoder → block output).
//
// Fragments are pre-encoded and MAC-framed *outside* the timed region and sent
// via UDP to the runtime's collector socket.  The timed region covers only
// the send → collect → verify → reassemble → decode → receive path.
// ============================================================================

/// Encode → feed all fragments (perfect reception) → decode.
fn benchmark_decoding_perfect_reception(c: &mut Criterion) {
    kaspa_core::log::try_init_logger("warn");
    let mut group = c.benchmark_group("decoding_perfect");
    // keep this heavy bench bounded so it doesn't run forever on CI/dev boxes
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("500KB", 500 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            let config = FragmentationConfig::new(16, 4, 1200);
            let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
            let transport = TransportParams { multiplier: 10.0, ..TransportParams::default() };
            let test_data = vec![42u8; size];
            let (send_socket, recv_addr, _runtime, block_rx, tokio_rt, _dir) = make_loopback_pipeline(config, &auth, transport);

            static ITER_CTR: AtomicU64 = AtomicU64::new(0);

            // measure each iteration separately; Criterion will loop as needed
            b.iter(|| {
                let iter_id = ITER_CTR.fetch_add(1, Ordering::Relaxed);
                let hash = random_hash();

                let fragments: Vec<Fragment> = FragmentGenerator::new(config, hash, FtrBlock(black_box(test_data.clone()))).collect();
                let frames = frame_fragments(&fragments, &auth);

                let start = Instant::now();
                for frame in black_box(&frames) {
                    let _ = send_socket.send_to(black_box(frame), recv_addr);
                }

                let rx = block_rx.clone();
                let received = tokio_rt.block_on(async {
                    let mut guard = rx.lock().await;
                    tokio::time::timeout(Duration::from_secs(10), guard.recv()).await
                });

                match received {
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => panic!("decoding_perfect: timeout or channel closed at iter {}", iter_id,),
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Encode → drop all parity fragments (data-only fast path) → decode.
fn benchmark_decoding_fast_path(c: &mut Criterion) {
    kaspa_core::log::try_init_logger("warn");

    let mut group = c.benchmark_group("decoding_fast_path");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            let config = FragmentationConfig::new(16, 4, 1200);
            let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
            let transport = TransportParams { multiplier: 10.0, ..TransportParams::default() };
            let test_data = vec![42u8; size];

            // create pipeline once per size
            let (send_socket, recv_addr, _runtime, block_rx, tokio_rt, _dir) = make_loopback_pipeline(config, &auth, transport);

            static ITER_CTR: AtomicU64 = AtomicU64::new(0);

            b.iter(|| {
                let iter_id = ITER_CTR.fetch_add(1, Ordering::Relaxed);
                let hash = random_hash();

                let fragments: Vec<Fragment> = FragmentGenerator::new(config, hash, FtrBlock(black_box(test_data.clone())))
                    .filter(|f| f.header().is_data(config.data_blocks as u16, config.parity_blocks as u16))
                    .collect();
                let frames = frame_fragments(&fragments, &auth);

                let start = Instant::now();
                for frame in black_box(&frames) {
                    let _ = send_socket.send_to(black_box(frame), recv_addr);
                }

                let received =
                    tokio_rt.block_on(async { tokio::time::timeout(Duration::from_secs(10), block_rx.lock().await.recv()).await });

                match received {
                    Ok(Some(_)) => (),
                    Ok(None) => panic!("decoding_fast_path: channel closed at iter {}", iter_id),
                    Err(_) => panic!("decoding_fast_path: timeout at iter {}", iter_id),
                }

                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Encode → drop one data fragment per generation (forces RS decode) → decode.
fn benchmark_decoding_with_loss(c: &mut Criterion) {
    kaspa_core::log::try_init_logger("warn");

    let mut group = c.benchmark_group("decoding_with_loss");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let data_sizes = vec![("64KB", 64 * 1024), ("256KB", 256 * 1024), ("1MB", 1024 * 1024)];

    for (label, size) in data_sizes {
        group.bench_with_input(BenchmarkId::from_parameter(label), &size, |b, &size| {
            let config = FragmentationConfig::new(4, 2, 1200);
            let gen_size = config.fragments_per_generation() as u16;
            let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
            let transport = TransportParams {
                k: config.data_blocks,
                m: config.parity_blocks,
                payload_size: config.payload_size,
                multiplier: 10.0,
                ..TransportParams::default()
            };
            let test_data = vec![42u8; size];

            let (send_socket, recv_addr, _runtime, block_rx, tokio_rt, _dir) = make_loopback_pipeline(config, &auth, transport);

            let mut block_guard = tokio_rt.block_on(async { block_rx.lock().await });

            static ITER_CTR: AtomicU64 = AtomicU64::new(0);

            b.iter(|| {
                let iter_id = ITER_CTR.fetch_add(1, Ordering::Relaxed);
                let hash = random_hash();

                let fragments: Vec<Fragment> = FragmentGenerator::new(config, hash, FtrBlock(black_box(test_data.clone())))
                    .filter(|f| f.header().index_within_generation(gen_size) != 0)
                    .collect();
                let frames = frame_fragments(&fragments, &auth);

                let start = Instant::now();
                for frame in black_box(&frames) {
                    let _ = send_socket.send_to(black_box(frame), recv_addr);
                }

                let deadline = start + Duration::from_secs(10);
                loop {
                    match block_guard.try_recv() {
                        Ok(msg) => {
                            black_box(msg.into_parts());
                            break;
                        }
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                            if Instant::now() > deadline {
                                panic!("decoding_with_loss: timeout at iter {}", iter_id);
                            }
                            std::thread::yield_now();
                        }
                        Err(_) => panic!("decoding_with_loss: channel closed at iter {}", iter_id),
                    }
                }

                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Vary k/m ratios with a fixed 500KB block.
fn benchmark_decoding_different_k_m(c: &mut Criterion) {
    kaspa_core::log::try_init_logger("warn");

    let mut group = c.benchmark_group("decoding_configs");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

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
            let config = FragmentationConfig::new(k, m, 1200);
            let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
            let transport = TransportParams { k, m, payload_size: 1200, multiplier: 10.0, ..TransportParams::default() };

            let (send_socket, recv_addr, _runtime, block_rx, tokio_rt, _dir) = make_loopback_pipeline(config, &auth, transport);

            let mut block_guard = tokio_rt.block_on(async { block_rx.lock().await });

            static ITER_CTR: AtomicU64 = AtomicU64::new(0);

            b.iter(|| {
                let iter_id = ITER_CTR.fetch_add(1, Ordering::Relaxed);
                let hash = random_hash();

                let fragments: Vec<Fragment> = FragmentGenerator::new(config, hash, FtrBlock(black_box(test_data.clone()))).collect();
                let frames = frame_fragments(&fragments, &auth);

                let start = Instant::now();
                for frame in black_box(&frames) {
                    let _ = send_socket.send_to(black_box(frame), recv_addr);
                }

                let deadline = start + Duration::from_secs(10);
                loop {
                    match block_guard.try_recv() {
                        Ok(msg) => {
                            black_box(msg.into_parts());
                            break;
                        }
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                            if Instant::now() > deadline {
                                panic!("decoding_configs: timeout at iter {}", iter_id);
                            }
                            std::thread::yield_now();
                        }
                        Err(_) => panic!("decoding_configs: channel closed at iter {}", iter_id),
                    }
                }

                start.elapsed()
            });
        });
    }

    group.finish();
}

// ============================================================================
// SERIALIZATION BENCHMARKS — Fragment wire format round-trip
// ============================================================================

/// Measure throughput of fragment serialization (to_bytes) and deserialization (from Bytes).
fn benchmark_fragment_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_serialization");
    let hash = Hash::from_bytes([0xEE; 32]);
    let header = FragmentHeader::new(hash, 42, 100);
    let payload = Bytes::from(vec![0xAB; 1200]);
    let fragment = Fragment::new(header, payload);
    let wire = fragment.to_bytes();

    group.bench_function("to_bytes", |b| {
        b.iter(|| {
            black_box(fragment.to_bytes());
        });
    });

    group.bench_function("from_bytes", |b| {
        b.iter(|| {
            black_box(Fragment::from(wire.clone()));
        });
    });

    group.finish();
}

// ============================================================================
// MAC / AUTHENTICATION BENCHMARKS
// ============================================================================

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

// ============================================================================
// UDP RECEIVE-LOOP BENCHMARK
// ============================================================================
//
// Exercises the ingest path: pre-framed UDP fragments → collector → verifier
// (MAC verify + dedup) → reassembler → decoded block.
//
// The old `UdpReceiveLoop` type was removed.  This benchmark now uses
// `TransportRuntime` to stand up the full pipeline and sends pre-framed
// packets via a raw UDP socket.

fn benchmark_udp_receive_loop(c: &mut Criterion) {
    kaspa_core::log::try_init_logger("warn");

    let mut group = c.benchmark_group("udp_receive_loop");

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let config = FragmentationConfig::new(16, 4, 1200);
    let transport = TransportParams { multiplier: 10.0, ..TransportParams::default() };

    const NUM_PACKETS: u16 = 100;
    const PAYLOAD_SIZE: usize = 1200;

    let (send_socket, recv_addr, _runtime, block_rx, tokio_rt, _dir) = make_loopback_pipeline(config, &auth, transport);

    static ITER_COUNTER: AtomicU64 = AtomicU64::new(0);

    group.bench_function("throughput_100_packets", |b| {
        b.iter_batched(
            || {
                ITER_COUNTER.fetch_add(1, Ordering::Relaxed);

                let mut frames = Vec::with_capacity(NUM_PACKETS as usize);
                // All packets belong to the same block so the pipeline can
                // reassemble and deliver a decoded block.
                let hash = Hash::from_bytes(rand::random::<[u8; 32]>());
                for pkt_idx in 0..NUM_PACKETS {
                    let header = FragmentHeader::new(hash, pkt_idx, NUM_PACKETS);
                    let fragment = Fragment::new(header, Bytes::from(vec![0u8; PAYLOAD_SIZE]));
                    let wire = fragment.to_bytes();
                    let mac = auth.mac(&wire);

                    let mut framed = Vec::with_capacity(mac.len() + wire.len());
                    framed.extend_from_slice(&mac);
                    framed.extend_from_slice(&wire);
                    frames.push(framed);
                }

                frames
            },
            |frames| {
                // Send all frames directly via blocking send_to.
                for frame in black_box(&frames) {
                    let _ = send_socket.send_to(black_box(frame), black_box(recv_addr));
                }

                // Wait for the decoded block to come through the pipeline.
                // The reassembler should produce a block once it has enough fragments.
                let rx = block_rx.clone();
                match tokio_rt.block_on(async {
                    let mut guard = rx.lock().await;
                    tokio::time::timeout(Duration::from_secs(4), guard.recv()).await
                }) {
                    Ok(Some(msg)) => {
                        black_box(msg.into_parts());
                    }
                    Ok(None) | Err(_) => {
                        eprintln!("udp_receive_loop: timeout/closed at iter {}", ITER_COUNTER.load(Ordering::Relaxed));
                    }
                }
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

// ============================================================================
// BROADCAST WORKER BENCHMARK — encode + MAC + send via UDP
// ============================================================================

fn benchmark_broadcast_worker_e2e_1mb(c: &mut Criterion) {
    let mut group = c.benchmark_group("broadcast_worker");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    // bench config
    let config = FragmentationConfig::new(16, 4, 1200);
    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());

    // 1 MB payload
    let data = vec![0u8; 1024 * 1024];

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
    let directory = Arc::new(PeerDirectory::new(HashMap::new()));
    let peer_info = PeerInfo::new(peer_addr, PeerDirection::Outbound, peer_addr).with_ready(true);
    directory.insert_peer(peer_info);
    // sanity-check directory contents
    let snap = directory.peer_info_list().load_full();
    assert_eq!(snap.len(), 1);
    assert!(snap[0].is_outbound_ready());

    // quick functional test: encode & send fragments directly (bypass broadcaster)
    {
        let sample_hash = Hash::from_bytes([0xAA; 32]);
        let expected_packets = FragmentGenerator::new(config, sample_hash, FtrBlock(data.clone())).count();
        let test_sender2 = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind test_sender");
        for fragment in FragmentGenerator::new(config, sample_hash, FtrBlock(data.clone())) {
            let wire = fragment.to_bytes();
            let mac = auth.mac(&wire);
            let mut framed = Vec::with_capacity(mac.len() + wire.len());
            framed.extend_from_slice(&mac);
            framed.extend_from_slice(&wire);
            test_sender2.send_to(&framed, peer_addr).expect("send fragment");
        }
        let mut drained = 0usize;
        while drained < expected_packets {
            let mut buf = [0u8; 4096];
            match peer_recv.recv_from(&mut buf) {
                Ok(_) => drained += 1,
                Err(_) => break,
            }
        }
        assert_eq!(drained, expected_packets, "direct send test should deliver all fragments");
    }

    // Create verification sender channels (broadcaster notifies verifiers about
    // blocks it has sent so they can mark them in dedup state)
    let mut verification_senders = Vec::with_capacity(1);
    let (vtx, _vrx) = crossbeam_channel::bounded::<VerificationMessage>(4096);
    verification_senders.push(vtx);

    // Create broadcast channel and spawn broadcaster thread
    let (broadcast_tx, broadcast_rx) = crossbeam_channel::bounded::<BroadcastMessage>(64);
    let _handle = spawn_broadcaster_thread(0, directory.clone(), broadcast_rx, Arc::new(auth.clone()), config, verification_senders);

    // Give the worker thread a short moment to initialize
    std::thread::sleep(Duration::from_millis(100));

    let sample_hash = Hash::from_bytes([0xAA; 32]);
    let expected_packets = FragmentGenerator::new(config, sample_hash, FtrBlock(data.clone())).total_fragments();

    println!("BroadcastWorker bench: sending {} fragments of size {} bytes", expected_packets, config.payload_size);

    // warm-up (best-effort)
    broadcast_tx.try_send(BroadcastMessage::new(sample_hash, Arc::new(FtrBlock(data.clone())))).expect("warmup");
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
        b.iter(|| {
            let hash = random_hash();
            let expected = FragmentGenerator::new(config, black_box(hash), FtrBlock(black_box(data.clone()))).total_fragments();
            peer_recv.set_read_timeout(Some(Duration::from_millis(100))).unwrap();

            let start = Instant::now();
            broadcast_tx
                .try_send(BroadcastMessage::new(black_box(hash), Arc::new(FtrBlock(black_box(data.clone())))))
                .expect("send job");

            // wait for all fragments to arrive at peer socket
            let mut received = 0usize;
            let deadline = Instant::now() + Duration::from_secs(10);
            while received < expected && Instant::now() < deadline {
                let mut buf = black_box([0u8; 4096]);
                match peer_recv.recv(&mut buf) {
                    Ok(_) => received += 1,
                    Err(_) => {
                        panic!(
                            "Timeout waiting for packets: received {}/{}: {:?}",
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

            start.elapsed()
        })
    });

    // teardown
    drop(broadcast_tx);
    drop(peer_recv);

    group.finish();
}

// ============================================================================
// PROCESSING-ONLY BENCHMARKS — verify MAC + parse Fragment (no syscalls)
// ============================================================================

/// Processing-only microbenchmark: verify MAC + parse Fragment from bytes (no syscalls).
fn benchmark_recv_path_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("recv_path_processing");

    let frames_to_generate = 1024usize;
    let payload_size = 1200usize;
    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());

    let mut frames: Vec<Vec<u8>> = Vec::with_capacity(frames_to_generate);
    for i in 0..frames_to_generate {
        let hash = Hash::from_bytes([(i & 0xFF) as u8; 32]);
        let header = FragmentHeader::new(hash, (i % 65535) as u16, 5);
        let fragment = Fragment::new(header, Bytes::from(vec![0xABu8; payload_size]));
        let wire = fragment.to_bytes();
        let mac = auth.mac(&wire);
        let mut framed = Vec::with_capacity(mac.len() + wire.len());
        framed.extend_from_slice(&mac);
        framed.extend_from_slice(&wire);
        frames.push(framed);
    }

    group.bench_function("process_packets", |b| {
        b.iter(|| {
            for frame in frames.iter().take(256) {
                let mac = &frame[..AuthToken::TOKEN_SIZE];
                let data = &frame[AuthToken::TOKEN_SIZE..];

                // Verify MAC
                debug_assert!(auth.verify_mac(data, mac));

                // Parse fragment (zero-copy into `Bytes` then `Fragment::from`)
                let bytes = Bytes::copy_from_slice(data);
                let fragment = Fragment::from(bytes);
                black_box(fragment.header().fragment_index());
            }
        });
    });

    group.finish();
}

// ============================================================================
// FULL-PIPELINE BENCHMARK
// ============================================================================
//
// Exercises the complete ingest path via TransportRuntime:
//   pre-framed UDP fragments → collector → verifier (MAC verify + dedup)
//   → reassembler (FEC decode) → decoded block received.
//
// The block is fragmented and MAC-framed *outside* the timed region so we
// measure only the runtime cost of: UDP recv → MAC verify → dedup →
// channel hop → reassembly → FEC decode → block delivery.
//
// Parameterised by `(collectors, verifiers, decoders, coordinators)` so we
// can sweep thread configurations in a single bench run.

fn benchmark_full_pipeline(c: &mut Criterion) {
    kaspa_core::log::try_init_logger("warn");

    let mut group = c.benchmark_group("full_pipeline");
    // Each iteration sends hundreds of packets — keep sample size modest.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let frag_config = FragmentationConfig::new(16, 4, 1200);

    const BLOCK_SIZE: usize = 500 * 1024; // 0.5 MB

    // Thread configurations to sweep: (label, collectors, verifiers, decoders_per_coord, coordinators)
    let configs: Vec<(&str, usize, usize, usize, usize)> = vec![
        ("1c_1v_1d_1c", 1, 1, 1, 1),
        ("1c_2v_2d_1c", 1, 2, 2, 1),
        ("2c_4v_2d_1c", 2, 4, 2, 1),
        ("2c_4v_4d_2c", 2, 4, 4, 2),
        ("4c_8v_4d_2c", 4, 8, 4, 2),
    ];

    // Iteration counter for unique hashes
    static PIPELINE_ITER: AtomicU64 = AtomicU64::new(0);

    for (label, num_collectors, num_verifiers, num_decoders, num_coordinators) in configs {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(num_collectors, num_verifiers, num_decoders, num_coordinators),
            |b, &(n_col, n_ver, n_dec, n_coord)| {
                // ---- infrastructure (created once per parameter set) ----
                let transport = TransportParams {
                    num_of_collectors: n_col,
                    num_of_verifiers: n_ver,
                    num_of_decoders_per_coordinators: n_dec,
                    num_of_coordinators: n_coord,
                    default_buffer_size: 1500,
                    multiplier: 10.0,
                    ..TransportParams::default()
                };

                let (send_socket, recv_addr, _runtime, block_rx, tokio_rt, _dir) =
                    make_loopback_pipeline(frag_config, &auth, transport);

                // ---- timed benchmark ----
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _i in 0..iters {
                        let _iter_id = PIPELINE_ITER.fetch_add(1, Ordering::Relaxed);
                        let hash = random_hash();

                        // Pre-encode block into framed UDP packets (outside measurement)
                        let test_data = vec![0x42u8; BLOCK_SIZE];
                        let fragments: Vec<Fragment> = FragmentGenerator::new(frag_config, hash, FtrBlock(test_data)).collect();
                        let frames = frame_fragments(&fragments, &auth);

                        // ---- TIMED: send → verify → decode → receive ----
                        let start = Instant::now();

                        for frame in &frames {
                            let _ = send_socket.send_to(frame, recv_addr);
                        }

                        let rx = block_rx.clone();
                        match tokio_rt.block_on(async {
                            let mut guard = rx.lock().await;
                            tokio::time::timeout(Duration::from_secs(10), guard.recv()).await
                        }) {
                            Ok(Some(msg)) => {
                                black_box(msg.into_parts());
                            }
                            Ok(None) | Err(_) => {
                                panic!("full_pipeline: timeout waiting for decoded block (iter {}, config {})", _i, label);
                            }
                        }

                        total += start.elapsed();
                    }

                    total
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// THROUGHPUT BENCHMARK  —  multiple blocks, lossy vs. lossless
// ============================================================================
//
// Fires `NUM_BLOCKS` blocks through the full ingest pipeline and measures the
// wall-clock time until **all** decoded blocks have been received.
//
// Two loss modes:
//   • **lossless** — every fragment is sent (tests fast-path / data-only decode).
//   • **lossy**    — one random *data* fragment per generation is dropped, forcing
//                    Reed-Solomon recovery for every generation.
//
// Parameterised by `(collectors, verifiers, decoders_per_coord, coordinators)`
// so we can sweep thread configurations in a single bench run.

fn benchmark_throughput(c: &mut Criterion) {
    use rand::Rng as _;
    kaspa_core::log::try_init_logger("warn");

    let mut group = c.benchmark_group("throughput");
    // We're sending many blocks — keep iterations low.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(5));

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let frag_config = FragmentationConfig::new(16, 4, 1200);

    const BLOCK_SIZE: usize = 500 * 1024; // 0.5 MB
    const NUM_BLOCKS: usize = 20;

    // Thread configurations: (label, collectors, verifiers, decoders_per_coord, coordinators)
    let thread_configs: Vec<(&str, usize, usize, usize, usize)> =
        vec![("1c_1v_1d_1c", 1, 1, 1, 1), ("2c_2v_2d_1c", 2, 2, 2, 1), ("2c_1v_2d_1c", 2, 1, 2, 1), ("4c_1v_4d_2c", 4, 1, 4, 2)];

    let loss_modes: Vec<(&str, bool)> = vec![("lossless", false), ("lossy", true)];

    for (tc_label, num_collectors, num_verifiers, num_decoders, num_coordinators) in &thread_configs {
        for (loss_label, lossy) in &loss_modes {
            let bench_id = format!("{}_{}_{tc_label}", NUM_BLOCKS, loss_label);

            group.bench_function(BenchmarkId::from_parameter(&bench_id), |b| {
                static THROUGHPUT_ITER: AtomicU64 = AtomicU64::new(0);

                let transport = TransportParams {
                    num_of_collectors: *num_collectors,
                    num_of_verifiers: *num_verifiers,
                    num_of_decoders_per_coordinators: *num_decoders,
                    num_of_coordinators: *num_coordinators,
                    default_buffer_size: 4096,
                    multiplier: 10.0,
                    ..TransportParams::default()
                };

                let (send_socket, recv_addr, _runtime, block_rx, tokio_rt, _dir) =
                    make_loopback_pipeline(frag_config, &auth, transport);

                let is_lossy = *lossy;

                // ---- timed benchmark ----
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _iter in 0..iters {
                        let _base = THROUGHPUT_ITER.fetch_add(NUM_BLOCKS as u64, Ordering::Relaxed);
                        let mut rng = rand::thread_rng();
                        let gen_size = frag_config.fragments_per_generation();

                        // Pre-encode all blocks outside timed section
                        let mut all_frames: Vec<Vec<Vec<u8>>> = Vec::with_capacity(NUM_BLOCKS);
                        for _blk in 0..NUM_BLOCKS {
                            let hash = random_hash();
                            let test_data = vec![0x42u8; BLOCK_SIZE];
                            let fragments: Vec<Fragment> = FragmentGenerator::new(frag_config, hash, FtrBlock(test_data)).collect();

                            let mut frames: Vec<Vec<u8>> = Vec::with_capacity(fragments.len());

                            let num_gens = fragments.len().div_ceil(gen_size);
                            let drop_per_gen: Vec<usize> = if is_lossy {
                                (0..num_gens)
                                    .map(|g| {
                                        let k = if g == num_gens - 1 {
                                            let rem = fragments.len() % gen_size;
                                            if rem == 0 { frag_config.data_blocks } else { rem.min(frag_config.data_blocks) }
                                        } else {
                                            frag_config.data_blocks
                                        };
                                        rng.gen_range(0..k)
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            for (idx, fragment) in fragments.iter().enumerate() {
                                if is_lossy {
                                    let g = idx / gen_size;
                                    let idx_in_gen = idx % gen_size;
                                    if idx_in_gen == drop_per_gen[g] {
                                        continue;
                                    }
                                }

                                let wire = fragment.to_bytes();
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
                            let rx = block_rx.clone();
                            match tokio_rt.block_on(async {
                                let mut guard = rx.lock().await;
                                tokio::time::timeout(Duration::from_secs(30), guard.recv()).await
                            }) {
                                Ok(Some(msg)) => {
                                    black_box(msg.into_parts());
                                    blocks_received += 1;
                                }
                                Ok(None) | Err(_) => {
                                    panic!(
                                        "throughput: timeout after {}/{} blocks \
                                         (config {}, {})",
                                        blocks_received, NUM_BLOCKS, tc_label, loss_label,
                                    );
                                }
                            }
                        }

                        total += start.elapsed();
                    }

                    total
                });
            });
        }
    }

    group.finish();
}

// ============================================================================
// CONGESTION BENCHMARK  —  N peers sending the *same* block
// ============================================================================
//
// Simulates multiple relay peers that all broadcast the same block (same hash,
// same fragments) simultaneously.  Only **one** decoded block should emerge.
//
// The verifiers' per-worker dedup ensures that only the first copy of each
// fragment goes through MAC verification, and all subsequent copies are
// rejected with a cheap bitset check *before* the expensive HMAC.
//
// Infrastructure is created once per parameter set (outside iter_custom).
// Pre-spawned sender threads wait on a `Barrier` for synchronized blasts.

fn benchmark_congestion(c: &mut Criterion) {
    use std::sync::Barrier;
    kaspa_core::log::try_init_logger("warn");

    let mut group = c.benchmark_group("congestion");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    let auth = TokenAuthenticator::new(b"bench-secret".to_vec());
    let frag_config = FragmentationConfig::new(16, 4, 1200);

    const BLOCK_SIZE: usize = 500 * 1024; // 0.5 MB

    // (label, num_collectors, num_peers, num_verifiers, num_decoders, num_coordinators)
    let configs: Vec<(&str, usize, usize, usize, usize, usize)> = vec![
        // We are generous with the coordinator and decoder threads, to ensure
        // these are not bottlenecks.
        // Congestion with same verifiers and collectors
        ("1c_12p_1v", 1, 24, 1, 8, 1),
        ("2c_12p_2v", 2, 24, 2, 8, 1),
        ("4c_12p_4v", 4, 24, 4, 8, 1),
        // Congestion with fewer verifiers
        ("2c_12p_1v", 2, 24, 1, 8, 1),
        ("4c_12p_1v", 4, 24, 2, 8, 1),
        ("8c_12p_1v", 4, 24, 1, 8, 1),
        // Congestion with few collectors
        ("1c_12p_2v", 1, 24, 2, 8, 1),
        ("2c_12p_4v", 2, 24, 4, 8, 1),
        ("1c_12p_4v", 1, 24, 4, 8, 1),
    ];

    // Iteration counter for unique hashes
    static CONGESTION_ITER: AtomicU64 = AtomicU64::new(0);

    for (label, num_collectors, num_peers, num_verifiers, num_decoders, num_coordinators) in configs {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(num_collectors, num_peers, num_verifiers, num_decoders, num_coordinators),
            |b, &(n_cols, n_peers, n_ver, n_dec, n_coord)| {
                // ---- infrastructure (created once per parameter set) ----
                let transport = TransportParams {
                    num_of_collectors: n_cols,
                    num_of_verifiers: n_ver,
                    num_of_decoders_per_coordinators: n_dec,
                    num_of_coordinators: n_coord,
                    default_buffer_size: 4096,
                    multiplier: 10.0,
                    ..TransportParams::default()
                };

                let listen_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
                let directory = Arc::new(PeerDirectory::new(HashMap::new()));
                let authenticator = Arc::new(auth.clone());

                let mut runtime = TransportRuntime::new(transport, listen_addr, frag_config, directory.clone(), authenticator.clone());
                runtime.start();

                let recv_addr = runtime.local_addr().expect("TransportRuntime bound address");

                let block_rx = runtime.block_receive();
                let tokio_rt = tokio::runtime::Runtime::new().unwrap();

                std::thread::sleep(Duration::from_millis(100));

                // Create N peer sender sockets, all allowlisted
                let allowlist = directory.allowlist();
                let peer_sockets: Vec<std::net::UdpSocket> = (0..n_peers)
                    .map(|_| {
                        let sock = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind peer");
                        set_recv_buf(&sock, 8 * 1024 * 1024);
                        let addr = sock.local_addr().expect("peer addr");
                        allowlist.rcu(|old| {
                            let mut s = (**old).clone();
                            s.insert(addr.ip(), PeerDirection::Both);
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
                        let _iter_id = CONGESTION_ITER.fetch_add(1, Ordering::Relaxed);
                        let hash = random_hash();

                        // Pre-encode one block (outside measurement)
                        let test_data = vec![0x42u8; BLOCK_SIZE];
                        let fragments: Vec<Fragment> = FragmentGenerator::new(frag_config, hash, FtrBlock(test_data)).collect();
                        let frames = frame_fragments(&fragments, &auth);

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
                        let rx = block_rx.clone();
                        match tokio_rt.block_on(async {
                            let mut guard = rx.lock().await;
                            tokio::time::timeout(Duration::from_secs(10), guard.recv()).await
                        }) {
                            Ok(Some(msg)) => {
                                black_box(msg.into_parts());
                            }
                            Ok(None) | Err(_) => {
                                panic!(
                                    "congestion: timeout waiting for decoded \
                                     block (config {}, {} peers)",
                                    label, n_peers
                                );
                            }
                        }

                        total += start.elapsed();

                        // Wait for all senders to finish (they may still be
                        // blasting duplicates after the block was decoded)
                        for _ in 0..n_peers {
                            let _ = done_rx.recv_timeout(Duration::from_secs(5));
                        }
                    }

                    total
                });

                // ---- Teardown ----
                drop(work_txs);
                std::thread::sleep(Duration::from_millis(50));
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    //benchmark_fragment_generation,
    //benchmark_fragment_generation_small,
    //benchmark_different_configs,
    benchmark_decoding_perfect_reception,
    //benchmark_decoding_fast_path,
    //benchmark_decoding_with_loss,
    //benchmark_decoding_different_k_m,
    //benchmark_fragment_serialization,
    //benchmark_mac_algorithms,
    //benchmark_udp_receive_loop,
    //benchmark_broadcast_worker_e2e_1mb,
    //benchmark_recv_path_processing,
    benchmark_full_pipeline,
    benchmark_throughput,
    benchmark_congestion,
);
criterion_main!(benches);
