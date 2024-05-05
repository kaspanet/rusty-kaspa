use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::future::join_all;
use kaspa_utils::sync::rwlock::{RfRwLock, RfRwLockOwnedReadGuard, RfRwLockOwnedWriteGuard};
use std::sync::Arc;
use tokio::sync::{OwnedRwLockWriteGuard as OwnedTokioRwLockWriteGuard, RwLock as TokioRwLock};

async fn run_many_readers<L: RwLockTrait<()> + Send + Sync + 'static>(n: usize)
where
    L::ReadGuard: Send + Sync,
{
    let l = Arc::new(L::new(()));
    let joins = (0..n).map(|_| {
        let l_clone = l.clone();
        tokio::spawn(async move { l_clone.read_().await })
    });
    join_all(joins).await;
}

fn bench_many_readers(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(24).enable_all().build().unwrap();
    let n = 100;

    c.bench_function("readers-preferred-rw-lock-non-fair-semaphore", |b| {
        #[allow(clippy::unit_arg)]
        b.iter(|| black_box(rt.block_on(run_many_readers::<Arc<RfRwLock>>(n))))
    });

    c.bench_function("readers-preferred-rw-lock-tokio-reference", |b| {
        #[allow(clippy::unit_arg)]
        b.iter(|| black_box(rt.block_on(run_many_readers::<Arc<TokioRwLock<()>>>(n))))
    });

    c.bench_function("readers-preferred-rw-lock-mutex-reference", |b| {
        #[allow(clippy::unit_arg)]
        b.iter(|| black_box(rt.block_on(run_many_readers::<ref_lock::RefReadersFirstRwLock<()>>(n))))
    });
}

criterion_group!(benches, bench_many_readers);
criterion_main!(benches);

#[async_trait::async_trait]
trait RwLockTrait<T> {
    type ReadGuard;
    type WriteGuard;
    fn new(value: T) -> Self;
    async fn read_(&self) -> Self::ReadGuard;
    #[allow(dead_code)]
    async fn write_(&self) -> Self::WriteGuard;
}

#[async_trait::async_trait]
impl<T: Send + Sync + 'static> RwLockTrait<T> for Arc<RfRwLock> {
    type ReadGuard = RfRwLockOwnedReadGuard;
    type WriteGuard = RfRwLockOwnedWriteGuard;

    fn new(_value: T) -> Self {
        Arc::new(RfRwLock::new())
    }

    async fn read_(&self) -> Self::ReadGuard {
        self.clone().read_owned().await
    }

    async fn write_(&self) -> Self::WriteGuard {
        self.clone().write_owned().await
    }
}

#[async_trait::async_trait]
impl<T: Send + Sync + 'static> RwLockTrait<T> for Arc<TokioRwLock<T>> {
    type ReadGuard = tokio::sync::OwnedRwLockReadGuard<T>;
    type WriteGuard = OwnedTokioRwLockWriteGuard<T>;

    fn new(value: T) -> Self {
        Arc::new(TokioRwLock::new(value))
    }

    async fn read_(&self) -> Self::ReadGuard {
        self.clone().read_owned().await
    }

    async fn write_(&self) -> Self::WriteGuard {
        self.clone().write_owned().await
    }
}

mod ref_lock {
    use crate::RwLockTrait;
    use std::sync::{Arc, Weak};
    use tokio::sync::{
        Mutex as TokioMutex, OwnedRwLockReadGuard as TokioOwnedRwLockReadGuard, OwnedRwLockWriteGuard as OwnedTokioRwLockWriteGuard,
        RwLock as TokioRwLock,
    };

    type RefReadersFirstRwLockReadGuard<T> = Arc<TokioOwnedRwLockReadGuard<T>>;

    pub struct RefReadersFirstRwLock<T> {
        inner: Arc<TokioRwLock<T>>,
        cached_readers_guard: TokioMutex<Option<Weak<TokioOwnedRwLockReadGuard<T>>>>,
    }

    impl<T> RefReadersFirstRwLock<T> {
        pub fn new(value: T) -> RefReadersFirstRwLock<T> {
            RefReadersFirstRwLock { inner: Arc::new(TokioRwLock::new(value)), cached_readers_guard: Default::default() }
        }

        pub async fn read(&self) -> RefReadersFirstRwLockReadGuard<T> {
            let mut g = self.cached_readers_guard.lock().await;
            if let Some(wrg) = g.clone() {
                if let Some(rg) = wrg.upgrade() {
                    return rg;
                }
            }
            let rg = Arc::new(self.inner.clone().read_owned().await);
            g.replace(Arc::downgrade(&rg));
            rg
        }

        pub async fn write(&self) -> OwnedTokioRwLockWriteGuard<T> {
            self.inner.clone().write_owned().await
        }
    }

    #[async_trait::async_trait]
    impl<T: Send + Sync> RwLockTrait<T> for RefReadersFirstRwLock<T> {
        type ReadGuard = RefReadersFirstRwLockReadGuard<T>;
        type WriteGuard = OwnedTokioRwLockWriteGuard<T>;

        fn new(value: T) -> Self {
            RefReadersFirstRwLock::new(value)
        }

        async fn read_(&self) -> Self::ReadGuard {
            self.read().await
        }

        async fn write_(&self) -> Self::WriteGuard {
            self.write().await
        }
    }
}
