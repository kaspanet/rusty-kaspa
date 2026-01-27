use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn benchmark_placeholder(_c: &mut Criterion) {
    black_box(0);
}

criterion_group!(benches, benchmark_placeholder);
criterion_main!(benches);
