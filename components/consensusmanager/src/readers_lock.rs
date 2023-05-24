use arc_swap::ArcSwapWeak;
use futures_util::{
    future::{BoxFuture, Shared},
    FutureExt,
};
use std::{
    ops::Deref,
    sync::{Arc, Weak},
};
use tokio::sync::{
    OwnedRwLockReadGuard as TokioOwnedRwLockReadGuard, RwLock as TokioRwLock, RwLockWriteGuard as TokioRwLockWriteGuard,
};

type ArcedOwnedRwLockReadGuard<T> = Arc<TokioOwnedRwLockReadGuard<T>>;
type FutureGuard<T> = BoxFuture<'static, ArcedOwnedRwLockReadGuard<T>>;
type SharedFutureGuard<T> = Shared<FutureGuard<T>>;

pub struct ReadersFirstRwLockReadGuard<T> {
    // Keeps the arc live so that other readers can obtain it from the cache
    _fut: Arc<SharedFutureGuard<T>>,
    guard: ArcedOwnedRwLockReadGuard<T>,
}

impl<T> Deref for ReadersFirstRwLockReadGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

pub struct ReadersFirstRwLock<T> {
    inner: Arc<TokioRwLock<T>>,
    cached_readers_guard: ArcSwapWeak<SharedFutureGuard<T>>,
}

impl<T: Send + Sync + 'static> ReadersFirstRwLock<T> {
    pub fn new(value: T) -> ReadersFirstRwLock<T> {
        ReadersFirstRwLock {
            inner: Arc::new(TokioRwLock::new(value)),
            cached_readers_guard: ArcSwapWeak::new(Weak::<SharedFutureGuard<T>>::new()),
        }
    }

    fn shared_read_inner(&self) -> SharedFutureGuard<T> {
        let bx: FutureGuard<T> = Box::pin(self.inner.clone().read_owned().map(Arc::new));
        bx.shared()
    }

    pub async fn read(&self) -> ReadersFirstRwLockReadGuard<T> {
        let mut weak = self.cached_readers_guard.load();
        loop {
            if let Some(guard) = weak.upgrade() {
                return ReadersFirstRwLockReadGuard { guard: (*guard).clone().await, _fut: guard };
            }
            let new_guard = Arc::new(self.shared_read_inner());
            let new_weak = Arc::downgrade(&new_guard);
            let prev_weak = self.cached_readers_guard.compare_and_swap(&weak, new_weak);
            if prev_weak.ptr_eq(&weak) {
                return ReadersFirstRwLockReadGuard { guard: (*new_guard).clone().await, _fut: new_guard };
            } else {
                weak = prev_weak;
            }
        }
    }

    pub fn blocking_read(&self) -> ReadersFirstRwLockReadGuard<T> {
        futures::executor::block_on(self.read())
    }

    pub async fn write(&self) -> TokioRwLockWriteGuard<'_, T> {
        self.inner.write().await
    }

    pub fn blocking_write(&self) -> TokioRwLockWriteGuard<'_, T> {
        self.inner.blocking_write()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::{sync::oneshot, time::timeout};

    #[tokio::test]
    async fn test_lock() {
        let l = Arc::new(ReadersFirstRwLock::new(()));
        let read1 = l.read().await;
        let read2 = l.read().await;
        let read3 = l.read().await;
        assert_eq!(4, Arc::strong_count(&read3.guard));
        println!("{}", Arc::strong_count(&read3.guard));
        println!("{:?}", read3._fut.strong_count());

        let (tx, rx) = oneshot::channel();
        let (tx_back, rx_back) = oneshot::channel();
        let l_clone = l.clone();
        let h = tokio::spawn(async move {
            let fut = l_clone.write();
            tx.send(()).unwrap();
            let _write = fut.await;
            rx_back.await.unwrap();
            println!("writer");
        });

        rx.await.unwrap();

        let read4 = timeout(Duration::from_secs(1), l.read()).await.unwrap();
        let read5 = timeout(Duration::from_secs(1), l.read()).await.unwrap();
        tx_back.send(()).unwrap();
        assert_eq!(6, Arc::strong_count(&read3.guard));
        println!("{}", Arc::strong_count(&read5.guard));
        println!("{:?}", read5._fut.strong_count());

        drop(read1);
        drop(read2);
        drop(read3);
        drop(read4);
        assert_eq!(2, Arc::strong_count(&read5.guard));
        println!("{}", Arc::strong_count(&read5.guard));
        println!("{:?}", read5._fut.strong_count());

        println!("dropping all readers");
        drop(read5);

        timeout(Duration::from_secs(1), h).await.unwrap().unwrap();
    }
}
