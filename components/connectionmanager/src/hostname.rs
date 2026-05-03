//! Hostname-origin connection state for the connection manager.
//!
//! Operator-pinned hostnames live in a sibling registry alongside the
//! `connection_requests` map. Resolution is async via a pluggable
//! [`HostnameResolver`] (production wires [`TokioHostnameResolver`]; tests
//! substitute a fake). Reconciliation is idempotent: a refresh tick that
//! returns the same socket-address set produces an empty
//! [`HostnameDelta`].

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use kaspa_utils::networking::{PeerEndpointResolveError, resolve_with_timeout};

/// Async DNS resolver dependency for the connection manager. Test seam:
/// production code uses [`TokioHostnameResolver`]; unit tests inject a
/// fake that returns a fixed result table.
#[async_trait]
pub trait HostnameResolver: Send + Sync + 'static {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, PeerEndpointResolveError>;
}

/// Production [`HostnameResolver`] backed by `tokio::net::lookup_host`
/// wrapped in `kaspa_utils::networking::resolve_with_timeout`. The
/// timeout bound and the `PeerEndpointResolveError` mapping are owned
/// by `kaspa-utils` so this impl and `PeerEndpoint::resolve` cannot
/// drift if the timeout constant or the error variant set changes.
#[derive(Default, Clone, Copy, Debug)]
pub struct TokioHostnameResolver;

#[async_trait]
impl HostnameResolver for TokioHostnameResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, PeerEndpointResolveError> {
        let target = format!("{host}:{port}");
        let lookup = async move {
            let iter = tokio::net::lookup_host(target).await?;
            Ok::<Vec<SocketAddr>, std::io::Error>(iter.collect())
        };
        resolve_with_timeout(host, lookup).await
    }
}

/// Per-hostname state retained across refresh cycles.
#[derive(Clone, Debug)]
pub struct HostnameRequest {
    pub host: Arc<str>,
    pub port: u16,
    pub is_permanent: bool,
    pub last_resolved: HashSet<SocketAddr>,
    pub last_refresh: SystemTime,
    pub refresh_failures: u32,
}

impl HostnameRequest {
    fn new(host: Arc<str>, port: u16, is_permanent: bool, initial: HashSet<SocketAddr>) -> Self {
        Self { host, port, is_permanent, last_resolved: initial, last_refresh: SystemTime::now(), refresh_failures: 0 }
    }
}

/// Per-host outcome produced by a parallel resolve phase, consumed by
/// [`HostnameRegistry::apply_refresh_results`].
pub type HostnameResolveOutcome = (Arc<str>, Result<Vec<SocketAddr>, PeerEndpointResolveError>);

/// Reconciliation outcome for a single host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostnameDelta {
    pub host: Arc<str>,
    pub added: Vec<SocketAddr>,
    pub removed: Vec<SocketAddr>,
}

impl HostnameDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Hostname-keyed state owned by the connection manager. The registry is
/// independent of the dial path: it owns the hostname-to-socket-addr
/// mapping, the refresh bookkeeping, and the dial-failure stale flag.
#[derive(Default)]
pub struct HostnameRegistry {
    requests: HashMap<Arc<str>, HostnameRequest>,
}

impl HostnameRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a hostname entry with its initial resolved set.
    /// Returns the inserted host's `Arc<str>` for back-references.
    ///
    /// Permissive on its own -- calls `HashMap::insert`, which silently
    /// overwrites any existing entry for `host`. The first-write-wins
    /// re-registration semantic that `ConnectionManager::add_endpoint_request`
    /// documents (`Re-registration semantics are first-write-wins`) is
    /// enforced at that single production call-site by gating this
    /// `upsert` behind a [`HostnameRegistry::contains`] check. Any
    /// future caller that needs first-write-wins MUST replicate that
    /// gate -- the method itself does not.
    pub fn upsert(&mut self, host: &str, port: u16, is_permanent: bool, initial: HashSet<SocketAddr>) -> Arc<str> {
        let key: Arc<str> = Arc::from(host);
        self.requests.insert(key.clone(), HostnameRequest::new(key.clone(), port, is_permanent, initial));
        key
    }

    /// Force the next periodic-refresh cadence check to treat `host` as
    /// eligible regardless of when it was last refreshed: sets
    /// `last_refresh` to [`UNIX_EPOCH`], so the cadence-elapsed predicate
    /// in [`HostnameRegistry::pending_refreshes`] returns `true` for it
    /// on the next tick. No-op if `host` is unknown.
    pub fn mark_stale(&mut self, host: &str) {
        if let Some(entry) = self.requests.get_mut(host) {
            entry.last_refresh = UNIX_EPOCH;
        }
    }

    pub fn contains(&self, host: &str) -> bool {
        self.requests.contains_key(host)
    }

    pub fn len(&self) -> usize {
        self.requests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn get(&self, host: &str) -> Option<&HostnameRequest> {
        self.requests.get(host)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Arc<str>, &HostnameRequest)> {
        self.requests.iter()
    }

    /// Snapshot the hosts whose `last_refresh + cadence` has elapsed
    /// (i.e. they are eligible to be re-resolved on the next periodic
    /// tick). Returned out-of-lock so the caller can run DNS lookups
    /// in parallel without holding the registry mutex across `await`
    /// points. Entries marked stale via [`mark_stale`] -- whose
    /// `last_refresh` is [`UNIX_EPOCH`] -- naturally satisfy the
    /// cadence-elapsed predicate and are always included.
    pub fn pending_refreshes(&self, cadence: Duration) -> Vec<(Arc<str>, u16, HashSet<SocketAddr>)> {
        let now = SystemTime::now();
        self.requests
            .iter()
            .filter(|(_, req)| now.duration_since(req.last_refresh).map(|elapsed| elapsed >= cadence).unwrap_or(true))
            .map(|(host, req)| (host.clone(), req.port, req.last_resolved.clone()))
            .collect()
    }

    /// Apply DNS results from a parallel resolve phase back into the
    /// registry. Called under the registry lock after the caller has
    /// run the resolves outside the lock. Bumps `refresh_failures` on
    /// `Err`; replaces `last_resolved` and resets `refresh_failures`
    /// on `Ok`. Returns one [`HostnameDelta`] per host with non-empty
    /// added/removed sets.
    pub fn apply_refresh_results(&mut self, results: Vec<HostnameResolveOutcome>) -> Vec<HostnameDelta> {
        let now = SystemTime::now();
        let mut deltas = Vec::with_capacity(results.len());
        for (host, result) in results {
            let prev = match self.requests.get(&host) {
                Some(req) => req.last_resolved.clone(),
                None => continue,
            };
            match result {
                Ok(resolved) => {
                    let new_set: HashSet<SocketAddr> = resolved.into_iter().collect();
                    let added: Vec<SocketAddr> = new_set.difference(&prev).copied().collect();
                    let removed: Vec<SocketAddr> = prev.difference(&new_set).copied().collect();
                    if let Some(entry) = self.requests.get_mut(&host) {
                        entry.last_resolved = new_set;
                        entry.last_refresh = now;
                        entry.refresh_failures = 0;
                    }
                    if !added.is_empty() || !removed.is_empty() {
                        deltas.push(HostnameDelta { host, added, removed });
                    }
                }
                Err(_e) => {
                    if let Some(entry) = self.requests.get_mut(&host) {
                        entry.refresh_failures = entry.refresh_failures.saturating_add(1);
                    }
                }
            }
        }
        deltas
    }

    /// Resolve every hostname entry through `resolver` and return the
    /// reconciliation deltas. Idempotent: an unchanged DNS result yields
    /// an empty delta. Resolution failures bump `refresh_failures` and
    /// leave `last_resolved` untouched -- the entry is retained even
    /// across many consecutive failures.
    ///
    /// Convenience entry that resolves every entry under a single
    /// `&mut self` borrow. Production callers prefer the
    /// [`Self::pending_refreshes`] / [`Self::apply_refresh_results`] pair so DNS
    /// lookups run outside the registry lock; this method is retained
    /// for unit-test ergonomics.
    pub async fn refresh_all<R: HostnameResolver + ?Sized>(&mut self, resolver: &R) -> Vec<HostnameDelta> {
        let snapshots: Vec<(Arc<str>, u16, HashSet<SocketAddr>)> =
            self.requests.iter().map(|(k, v)| (k.clone(), v.port, v.last_resolved.clone())).collect();
        let mut deltas = Vec::with_capacity(snapshots.len());
        for (host, port, prev) in snapshots {
            match resolver.resolve(&host, port).await {
                Ok(resolved) => {
                    let new_set: HashSet<SocketAddr> = resolved.into_iter().collect();
                    let added: Vec<SocketAddr> = new_set.difference(&prev).copied().collect();
                    let removed: Vec<SocketAddr> = prev.difference(&new_set).copied().collect();
                    if let Some(entry) = self.requests.get_mut(&host) {
                        entry.last_resolved = new_set;
                        entry.last_refresh = SystemTime::now();
                        entry.refresh_failures = 0;
                    }
                    if !added.is_empty() || !removed.is_empty() {
                        deltas.push(HostnameDelta { host: host.clone(), added, removed });
                    }
                }
                Err(_e) => {
                    if let Some(entry) = self.requests.get_mut(&host) {
                        entry.refresh_failures = entry.refresh_failures.saturating_add(1);
                    }
                }
            }
        }
        deltas
    }

    /// Return the snapshot of `(host, last_resolved)` pairs needed by the
    /// dial-failure handler to decide which hostname owns a given socket.
    /// Crate-internal: the production back-reference uses
    /// `ConnectionRequest.hostname_origin` directly; this method exists
    /// for the registry-side authoritative inverse and is exercised by
    /// unit tests of the registry's reverse map.
    pub(crate) fn host_for_socket(&self, addr: &SocketAddr) -> Option<Arc<str>> {
        self.requests.iter().find_map(|(host, req)| if req.last_resolved.contains(addr) { Some(host.clone()) } else { None })
    }
}

/// Helper used by the periodic refresh wakeup window. Returns `true` if
/// `interval` is non-zero, signalling the refresh task should be spawned.
pub fn refresh_enabled(interval: Duration) -> bool {
    !interval.is_zero()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex;

    /// Programmable fake [`HostnameResolver`]. Each entry maps a
    /// `(host, port)` key to either a fixed list of socket addresses or
    /// an error message. `mutate` swaps the response between calls.
    struct FakeResolver {
        table: Mutex<StdHashMap<String, Result<Vec<SocketAddr>, String>>>,
    }

    impl FakeResolver {
        fn new() -> Self {
            Self { table: Mutex::new(StdHashMap::new()) }
        }

        fn set(&self, host: &str, port: u16, value: Result<Vec<SocketAddr>, String>) {
            self.table.lock().unwrap().insert(format!("{host}:{port}"), value);
        }
    }

    #[async_trait]
    impl HostnameResolver for FakeResolver {
        async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, PeerEndpointResolveError> {
            let key = format!("{host}:{port}");
            let table = self.table.lock().unwrap();
            match table.get(&key) {
                Some(Ok(addrs)) => Ok(addrs.clone()),
                Some(Err(reason)) => {
                    Err(PeerEndpointResolveError::Lookup { host: host.to_owned(), source: std::io::Error::other(reason.clone()) })
                }
                None => Err(PeerEndpointResolveError::Lookup {
                    host: host.to_owned(),
                    source: std::io::Error::new(std::io::ErrorKind::NotFound, format!("no fake entry for {key}")),
                }),
            }
        }
    }

    fn sock(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    #[tokio::test]
    async fn registry_upsert_inserts_initial_set() {
        let mut reg = HostnameRegistry::new();
        let initial: HashSet<SocketAddr> = [sock("10.0.0.1:16111")].into_iter().collect();
        reg.upsert("a.example", 16111, true, initial.clone());
        assert!(reg.contains("a.example"));
        assert_eq!(reg.get("a.example").unwrap().last_resolved, initial);
        assert_eq!(reg.get("a.example").unwrap().refresh_failures, 0);
    }

    #[tokio::test]
    async fn registry_refresh_observes_added_ip() {
        let mut reg = HostnameRegistry::new();
        let prev: HashSet<SocketAddr> = [sock("10.0.0.1:16111")].into_iter().collect();
        reg.upsert("a.example", 16111, true, prev);
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.1:16111"), sock("10.0.0.2:16111")]));
        let deltas = reg.refresh_all(&resolver).await;
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].added, vec![sock("10.0.0.2:16111")]);
        assert!(deltas[0].removed.is_empty());
        let entry = reg.get("a.example").unwrap();
        assert_eq!(entry.last_resolved.len(), 2);
        assert_eq!(entry.refresh_failures, 0);
    }

    #[tokio::test]
    async fn registry_refresh_observes_removed_ip() {
        let mut reg = HostnameRegistry::new();
        let prev: HashSet<SocketAddr> = [sock("10.0.0.1:16111"), sock("10.0.0.2:16111")].into_iter().collect();
        reg.upsert("a.example", 16111, true, prev);
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.1:16111")]));
        let deltas = reg.refresh_all(&resolver).await;
        assert_eq!(deltas.len(), 1);
        assert!(deltas[0].added.is_empty());
        assert_eq!(deltas[0].removed, vec![sock("10.0.0.2:16111")]);
        let entry = reg.get("a.example").unwrap();
        assert_eq!(entry.last_resolved.len(), 1);
    }

    #[tokio::test]
    async fn registry_refresh_total_churn() {
        let mut reg = HostnameRegistry::new();
        let prev: HashSet<SocketAddr> = [sock("10.0.0.1:16111"), sock("10.0.0.2:16111")].into_iter().collect();
        reg.upsert("a.example", 16111, true, prev);
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.3:16111"), sock("10.0.0.4:16111")]));
        let deltas = reg.refresh_all(&resolver).await;
        assert_eq!(deltas.len(), 1);
        let mut added = deltas[0].added.clone();
        let mut removed = deltas[0].removed.clone();
        added.sort();
        removed.sort();
        assert_eq!(added, vec![sock("10.0.0.3:16111"), sock("10.0.0.4:16111")]);
        assert_eq!(removed, vec![sock("10.0.0.1:16111"), sock("10.0.0.2:16111")]);
    }

    #[tokio::test]
    async fn registry_refresh_idempotent_when_unchanged() {
        let mut reg = HostnameRegistry::new();
        let prev: HashSet<SocketAddr> = [sock("10.0.0.1:16111")].into_iter().collect();
        reg.upsert("a.example", 16111, true, prev);
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.1:16111")]));
        let deltas = reg.refresh_all(&resolver).await;
        assert!(deltas.is_empty(), "expected empty delta for unchanged DNS, got {deltas:?}");
        let deltas = reg.refresh_all(&resolver).await;
        assert!(deltas.is_empty());
    }

    #[tokio::test]
    async fn registry_refresh_failure_does_not_drop_entry() {
        let mut reg = HostnameRegistry::new();
        let prev: HashSet<SocketAddr> = [sock("10.0.0.1:16111")].into_iter().collect();
        reg.upsert("a.example", 16111, true, prev);
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Err("synthetic DNS failure".to_owned()));
        let deltas = reg.refresh_all(&resolver).await;
        assert!(deltas.is_empty(), "no delta emitted on failure, got {deltas:?}");
        let entry = reg.get("a.example").unwrap();
        assert_eq!(entry.refresh_failures, 1);
        assert_eq!(entry.last_resolved.len(), 1, "last_resolved must be untouched on failure");
    }

    #[tokio::test]
    async fn registry_permanent_hostname_survives_consecutive_failures() {
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, HashSet::new());
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Err("transient".to_owned()));
        for _ in 0..5 {
            reg.refresh_all(&resolver).await;
        }
        let entry = reg.get("a.example").unwrap();
        assert!(entry.is_permanent);
        assert_eq!(entry.refresh_failures, 5);
        assert!(reg.contains("a.example"));
    }

    #[tokio::test]
    async fn registry_multi_record_resolution_reports_all_added() {
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, HashSet::new());
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.1:16111"), sock("10.0.0.2:16111"), sock("10.0.0.3:16111")]));
        let deltas = reg.refresh_all(&resolver).await;
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].added.len(), 3);
        assert!(deltas[0].removed.is_empty());
    }

    #[tokio::test]
    async fn registry_mark_stale_zeroes_last_refresh() {
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, [sock("10.0.0.1:16111")].into_iter().collect());
        let before = reg.get("a.example").unwrap().last_refresh;
        reg.mark_stale("a.example");
        let after = reg.get("a.example").unwrap().last_refresh;
        assert!(after < before, "mark_stale must move last_refresh backward");
        assert_eq!(after, UNIX_EPOCH);
        // Marking an unknown host is a no-op.
        reg.mark_stale("not.in.registry");
    }

    #[tokio::test]
    async fn registry_host_for_socket_round_trips() {
        let mut reg = HostnameRegistry::new();
        let s1 = sock("10.0.0.1:16111");
        let s2 = sock("10.0.0.2:16111");
        reg.upsert("a.example", 16111, true, [s1].into_iter().collect());
        reg.upsert("b.example", 16111, false, [s2].into_iter().collect());
        assert_eq!(reg.host_for_socket(&s1).as_deref(), Some("a.example"));
        assert_eq!(reg.host_for_socket(&s2).as_deref(), Some("b.example"));
        assert!(reg.host_for_socket(&sock("10.0.0.99:16111")).is_none());
    }

    #[tokio::test]
    async fn registry_refresh_clears_failures_on_success() {
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, HashSet::new());
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Err("first miss".to_owned()));
        reg.refresh_all(&resolver).await;
        assert_eq!(reg.get("a.example").unwrap().refresh_failures, 1);
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.1:16111")]));
        reg.refresh_all(&resolver).await;
        assert_eq!(reg.get("a.example").unwrap().refresh_failures, 0);
    }

    #[test]
    fn refresh_enabled_zero_means_disabled() {
        assert!(!refresh_enabled(Duration::ZERO));
        assert!(refresh_enabled(Duration::from_secs(1)));
        assert!(refresh_enabled(Duration::from_millis(1)));
    }

    #[tokio::test]
    async fn registry_refresh_failure_independent_per_host() {
        let mut reg = HostnameRegistry::new();
        reg.upsert("ok.example", 16111, true, HashSet::new());
        reg.upsert("bad.example", 16111, true, HashSet::new());
        let resolver = FakeResolver::new();
        resolver.set("ok.example", 16111, Ok(vec![sock("10.0.0.1:16111")]));
        resolver.set("bad.example", 16111, Err("synthetic".to_owned()));
        reg.refresh_all(&resolver).await;
        assert_eq!(reg.get("ok.example").unwrap().refresh_failures, 0);
        assert_eq!(reg.get("ok.example").unwrap().last_resolved.len(), 1);
        assert_eq!(reg.get("bad.example").unwrap().refresh_failures, 1);
        assert!(reg.get("bad.example").unwrap().last_resolved.is_empty());
    }
}
