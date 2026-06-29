//! Helpers for awaiting `tokio` blocking-task joins during normal operation and shutdown.

use tokio::task::{JoinError, JoinHandle};

/// Resolves the outcome of awaiting a `spawn_blocking` join (or a combinator such as
/// `try_join_all` over such joins), handling the two [`JoinError`] cases:
///
/// - **Cancellation** only happens while the Tokio runtime is shutting down, where there is no
///   result to return. Rather than panicking (which crashes a worker thread mid-teardown), park
///   until this task is dropped as part of runtime teardown.
/// - **Panic** inside the spawned closure is propagated to the caller, preserving the behavior of
///   the previous `.unwrap()` while re-raising the original panic payload.
pub async fn join_result_or_park<T>(joined: Result<T, JoinError>) -> T {
    match joined {
        Ok(value) => value,
        // A cancelled blocking task only occurs on runtime shutdown; there is nothing to return,
        // so park until teardown drops this task instead of panicking.
        Err(err) if err.is_cancelled() => std::future::pending().await,
        // Propagate a genuine panic from the spawned closure.
        Err(err) => std::panic::resume_unwind(err.into_panic()),
    }
}

/// Awaits a `spawn_blocking` join handle, parking on shutdown cancellation and propagating a
/// closure panic. See [`join_result_or_park`].
pub async fn join_blocking_or_park<R>(handle: JoinHandle<R>) -> R {
    join_result_or_park(handle.await).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread().worker_threads(2).build().unwrap()
    }

    #[test]
    fn returns_closure_result() {
        let rt = runtime();
        let value = rt.block_on(async { join_blocking_or_park(tokio::task::spawn_blocking(|| 42u32)).await });
        assert_eq!(value, 42);
    }

    #[test]
    fn parks_on_cancellation() {
        // A cancelled join handle (the runtime-shutdown case) must park rather than panic or resolve,
        // since there is no result to return. We emulate the cancellation by aborting a task.
        let rt = runtime();
        rt.block_on(async {
            let cancelled = tokio::spawn(std::future::pending::<u32>());
            cancelled.abort();

            let parked = tokio::spawn(join_blocking_or_park(cancelled));
            for _ in 0..1000 {
                tokio::task::yield_now().await;
            }
            assert!(!parked.is_finished(), "helper should park on cancellation rather than resolve");
            parked.abort();
        });
    }

    #[test]
    fn propagates_closure_panic() {
        // A genuine panic inside the closure must propagate to the caller (not be swallowed).
        let rt = runtime();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.block_on(async { join_blocking_or_park(tokio::task::spawn_blocking(|| -> u32 { panic!("boom-{}", 42) })).await })
        }));
        let payload = result.expect_err("closure panic should propagate through join_blocking_or_park");
        let message =
            payload.downcast_ref::<String>().map(String::as_str).or_else(|| payload.downcast_ref::<&str>().copied()).unwrap_or("");
        assert!(message.contains("boom-42"), "panic payload should be preserved, got: {message:?}");
    }
}
