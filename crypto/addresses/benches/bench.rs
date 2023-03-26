use criterion::{black_box, criterion_group, criterion_main, Criterion};

use kaspa_addresses::{Address, Prefix};

pub fn encode_benchmark(c: &mut Criterion) {
    c.bench_function("Address::into::String", |b| {
        let address = Address::new(
            Prefix::Mainnet,
            kaspa_addresses::Version::PubKey,
            b"\x5f\xff\x3c\x4d\xa1\x8f\x45\xad\xcd\xd4\x99\xe4\x46\x11\xe9\xff\xf1\x48\xba\x69\xdb\x3c\x4e\xa2\xdd\xd9\x55\xfc\x46\xa5\x95\x22",
        );
        b.iter(|| -> String { Address::into(black_box(address.clone())) })
    });
}

pub fn decode_benchmark(c: &mut Criterion) {
    c.bench_function("String::into::Address", |b| {
        let address = "kaspa:qp0l70zd5x85ttwd6jv7g3s3a8llzj96d8dncn4zmhv4tlzx5k2jyqh70xmfj".to_string();
        b.iter(|| -> Address { String::try_into(black_box(address.clone())).expect("Should work") })
    });
}

criterion_group!(benches, encode_benchmark, decode_benchmark);
criterion_main!(benches);
