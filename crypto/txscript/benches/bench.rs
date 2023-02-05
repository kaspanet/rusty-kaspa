use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_placeholder(_c: &mut Criterion) {
    black_box(0);
}

criterion_group!(benches, benchmark_placeholder);
criterion_main!(benches);
