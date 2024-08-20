use arc_swap::ArcSwapOption;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

struct Entry<T> {
    item: T,
    timestamp: Instant,
}

/// An expiring cache for a single object
pub struct ExpiringCache<T> {
    store: ArcSwapOption<Entry<T>>,
    refetch: Duration,
    expire: Duration,
    fetching: AtomicBool,
}

impl<T: Clone> ExpiringCache<T> {
    /// Constructs a new expiring cache where `fetch` is the amount of time required to trigger a data
    /// refetch and `expire` is the time duration after which the stored item is guaranteed not to be returned.
    ///
    /// Panics if `refetch > expire`.
    pub fn new(refetch: Duration, expire: Duration) -> Self {
        assert!(refetch <= expire);
        Self { store: Default::default(), refetch, expire, fetching: Default::default() }
    }

    /// Returns the cached item or possibly fetches a new one using the `refetch_future` task. The
    /// decision whether to refetch depends on the configured expiration and refetch times for this cache.  
    pub async fn get<F>(&self, refetch_future: F) -> T
    where
        F: Future<Output = T> + Send + 'static,
        F::Output: Send + 'static,
    {
        let mut fetching = false;

        {
            let guard = self.store.load();
            if let Some(entry) = guard.as_ref() {
                if let Some(elapsed) = Instant::now().checked_duration_since(entry.timestamp) {
                    if elapsed < self.refetch {
                        return entry.item.clone();
                    }
                    // Refetch is triggered, attempt to capture the task
                    fetching = self.fetching.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok();
                    // If the fetch task is not captured and expire time is not over yet, return with prev value. Another
                    // thread is refetching the data but we can return with the not-too-old value
                    if !fetching && elapsed < self.expire {
                        return entry.item.clone();
                    }
                }
                // else -- In rare cases where now < timestamp, fall through to re-update the cache
            }
        }

        // We reach here if either we are the refetching thread or the current data has fully expired
        let new_item = refetch_future.await;
        let timestamp = Instant::now();
        // Update the store even if we were not in charge of refetching - let the last thread make the final update
        self.store.store(Some(Arc::new(Entry { item: new_item.clone(), timestamp })));

        if fetching {
            let result = self.fetching.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst);
            assert!(result.is_ok(), "refetching was captured")
        }

        new_item
    }
}

#[cfg(test)]
mod tests {
    use super::ExpiringCache;
    use std::time::Duration;
    use tokio::join;

    #[tokio::test]
    #[ignore]
    // Tested during development but can be sensitive to runtime machine times so there's no point
    // in keeping it part of CI. The test should be activated if the ExpiringCache struct changes.
    async fn test_expiring_cache() {
        let fetch = Duration::from_millis(500);
        let expire = Duration::from_millis(1000);
        let mid_point = Duration::from_millis(700);
        let expire_point = Duration::from_millis(1200);
        let cache: ExpiringCache<u64> = ExpiringCache::new(fetch, expire);

        // Test two consecutive calls
        let item1 = cache
            .get(async move {
                println!("first call");
                1
            })
            .await;
        assert_eq!(1, item1);
        let item2 = cache
            .get(async move {
                // cache was just updated with item1, refetch should not be triggered
                panic!("should not be called");
            })
            .await;
        assert_eq!(1, item2);

        // Test two calls after refetch point
        // Sleep until after the refetch point but before expire
        tokio::time::sleep(mid_point).await;
        let call3 = cache.get(async move {
            println!("third call before sleep");
            // keep this refetch busy so that call4 still gets the first item
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("third call after sleep");
            3
        });
        let call4 = cache.get(async move {
            // refetch is captured by call3 and we should be before expire
            panic!("should not be called");
        });
        let (item3, item4) = join!(call3, call4);
        println!("item 3: {}, item 4: {}", item3, item4);
        assert_eq!(3, item3);
        assert_eq!(1, item4);

        // Test 2 calls after expire
        tokio::time::sleep(expire_point).await;
        let call5 = cache.get(async move {
            println!("5th call before sleep");
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("5th call after sleep");
            5
        });
        let call6 = cache.get(async move { 6 });
        let (item5, item6) = join!(call5, call6);
        println!("item 5: {}, item 6: {}", item5, item6);
        assert_eq!(5, item5);
        assert_eq!(6, item6);

        let item7 = cache
            .get(async move {
                // cache was just updated with item5, refetch should not be triggered
                panic!("should not be called");
            })
            .await;
        // call 5 finished after call 6
        assert_eq!(5, item7);
    }
}
