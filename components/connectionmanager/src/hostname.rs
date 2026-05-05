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
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use kaspa_utils::networking::{PeerEndpointResolveError, resolve_with_timeout};
use tokio::time::Instant;

/// What triggered a hostname resolution attempt. Captured as a metric label.
///
/// `Initial` and `InitialRetry` together cover the registration window:
/// a successful first resolve emits `Initial`; the early retry on a
/// register-with-failed-resolve entry (one that has never resolved
/// successfully) emits `InitialRetry`. `DialFailure` is reserved for
/// entries the dial loop flagged stale after a dial against a
/// previously-resolved socket failed. `Periodic` is the cadence-elapsed
/// re-resolve of an entry that DID previously resolve successfully.
/// Operators monitoring `dial_failure_*` get a clean signal -- it never
/// aggregates the never-resolved-yet path.
///
/// Marked `#[non_exhaustive]` so additional trigger labels (e.g. an
/// operator-driven `ForceRefresh`, a future `RotationCadence`) can be
/// added in a later release without a SemVer-major break for downstream
/// matchers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResolveTrigger {
    /// First resolution at registration time (operator added the entry).
    Initial,
    /// Early-retry on an entry registered without an initial successful
    /// resolve (DNS empty / lookup error at registration time). Distinct
    /// from `DialFailure` so the metric label semantics match operator
    /// intuition: this bucket tracks "host never resolved yet", whereas
    /// `DialFailure` tracks "host previously resolved, dial against a
    /// resolved socket failed".
    InitialRetry,
    /// Re-resolution after a dial against a previously-resolved socket failed.
    DialFailure,
    /// Periodic background re-resolution at the configured cadence.
    Periodic,
}

impl ResolveTrigger {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::InitialRetry => "initial_retry",
            Self::DialFailure => "dial_failure",
            Self::Periodic => "periodic",
        }
    }
}

/// Why an entry's `last_refresh` was zeroed.
///
/// Drives the per-entry [`ResolveTrigger`] yielded by
/// [`HostnameRegistry::pending_refreshes`] when the entry is picked up
/// for re-resolution: `InitialRetry` for the register-with-failed-resolve
/// path, `DialFailure` for the dial-loop-flagged path, `PeriodicEmpty`
/// for the periodic-resolve-returned-empty path.
///
/// Marked `#[non_exhaustive]` so additional reasons (e.g. an
/// operator-driven `ManualRefresh`, a future
/// `RotationCadenceFailure`) can be added in a later release without
/// a SemVer-major break for downstream matchers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StaleReason {
    /// Set by `add_endpoint_request` when a hostname is registered with
    /// an empty / errored initial resolve. The entry has never resolved
    /// successfully; the next refresh tick retries immediately rather
    /// than waiting the full cadence.
    InitialRetry,
    /// Set by `handle_connection_requests` when a dial against a socket
    /// owned by a hostname-origin entry failed. The entry has resolved
    /// successfully at least once; the next refresh tick re-resolves so
    /// rotated DNS picks up the new IPs.
    DialFailure,
    /// Set by `apply_refresh_results` (and `refresh_all`) when a
    /// periodic re-resolution returned `Ok(empty)`. The entry has zero
    /// resolved IPs after the apply; the next refresh tick re-resolves
    /// immediately rather than waiting the full cadence, mirroring the
    /// `add_endpoint_request` initial-empty fast-retry path. The next
    /// tick's metric label remains [`ResolveTrigger::Periodic`] (this
    /// IS a periodic re-resolution, just one accelerated by the empty
    /// outcome) rather than mislabeling under `dial_failure_*` or
    /// `initial_retry_*`.
    PeriodicEmpty,
}

/// Outcome of a hostname resolution attempt. Captured as a metric label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolveStatus {
    Ok,
    Failed,
}

impl ResolveStatus {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }
}

/// Counter buckets for `peer_hostname_resolutions_total`. Eight combinations:
/// `{ok,failed} x {initial, initial_retry, dial_failure, periodic}`.
#[derive(Default, Debug)]
pub struct HostnameMetrics {
    initial_ok: AtomicU64,
    initial_failed: AtomicU64,
    initial_retry_ok: AtomicU64,
    initial_retry_failed: AtomicU64,
    dial_failure_ok: AtomicU64,
    dial_failure_failed: AtomicU64,
    periodic_ok: AtomicU64,
    periodic_failed: AtomicU64,
}

impl HostnameMetrics {
    pub fn record(&self, trigger: ResolveTrigger, status: ResolveStatus) {
        let counter = match (trigger, status) {
            (ResolveTrigger::Initial, ResolveStatus::Ok) => &self.initial_ok,
            (ResolveTrigger::Initial, ResolveStatus::Failed) => &self.initial_failed,
            (ResolveTrigger::InitialRetry, ResolveStatus::Ok) => &self.initial_retry_ok,
            (ResolveTrigger::InitialRetry, ResolveStatus::Failed) => &self.initial_retry_failed,
            (ResolveTrigger::DialFailure, ResolveStatus::Ok) => &self.dial_failure_ok,
            (ResolveTrigger::DialFailure, ResolveStatus::Failed) => &self.dial_failure_failed,
            (ResolveTrigger::Periodic, ResolveStatus::Ok) => &self.periodic_ok,
            (ResolveTrigger::Periodic, ResolveStatus::Failed) => &self.periodic_failed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> HostnameMetricsSnapshot {
        HostnameMetricsSnapshot {
            resolutions_total: ResolutionsTotal {
                initial_ok: self.initial_ok.load(Ordering::Relaxed),
                initial_failed: self.initial_failed.load(Ordering::Relaxed),
                initial_retry_ok: self.initial_retry_ok.load(Ordering::Relaxed),
                initial_retry_failed: self.initial_retry_failed.load(Ordering::Relaxed),
                dial_failure_ok: self.dial_failure_ok.load(Ordering::Relaxed),
                dial_failure_failed: self.dial_failure_failed.load(Ordering::Relaxed),
                periodic_ok: self.periodic_ok.load(Ordering::Relaxed),
                periodic_failed: self.periodic_failed.load(Ordering::Relaxed),
            },
            // Gauges are populated by the caller from the live registry.
            active: 0,
            resolved_addrs: 0,
        }
    }

    pub fn get(&self, trigger: ResolveTrigger, status: ResolveStatus) -> u64 {
        let counter = match (trigger, status) {
            (ResolveTrigger::Initial, ResolveStatus::Ok) => &self.initial_ok,
            (ResolveTrigger::Initial, ResolveStatus::Failed) => &self.initial_failed,
            (ResolveTrigger::InitialRetry, ResolveStatus::Ok) => &self.initial_retry_ok,
            (ResolveTrigger::InitialRetry, ResolveStatus::Failed) => &self.initial_retry_failed,
            (ResolveTrigger::DialFailure, ResolveStatus::Ok) => &self.dial_failure_ok,
            (ResolveTrigger::DialFailure, ResolveStatus::Failed) => &self.dial_failure_failed,
            (ResolveTrigger::Periodic, ResolveStatus::Ok) => &self.periodic_ok,
            (ResolveTrigger::Periodic, ResolveStatus::Failed) => &self.periodic_failed,
        };
        counter.load(Ordering::Relaxed)
    }
}

/// Aggregated counter values for `peer_hostname_resolutions_total`.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolutionsTotal {
    pub initial_ok: u64,
    pub initial_failed: u64,
    pub initial_retry_ok: u64,
    pub initial_retry_failed: u64,
    pub dial_failure_ok: u64,
    pub dial_failure_failed: u64,
    pub periodic_ok: u64,
    pub periodic_failed: u64,
}

/// Point-in-time snapshot suitable for export to Prometheus / RPC. The
/// `active` and `resolved_addrs` gauges are filled by the producer at
/// snapshot time from the live `HostnameRegistry`.
///
/// The [`resolutions_total`](Self::resolutions_total) field type
/// [`ResolutionsTotal`] is reachable only via the
/// `kaspa_connectionmanager::hostname::ResolutionsTotal` path, not at
/// the crate root. Downstream code that constructs or pattern-matches
/// the field must import from the submodule path; the snapshot type
/// is the only crate-root re-export of the metrics surface.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostnameMetricsSnapshot {
    pub resolutions_total: ResolutionsTotal,
    pub active: u64,
    pub resolved_addrs: u64,
}

/// Async DNS resolver dependency for the connection manager. Test seam:
/// production code uses [`TokioHostnameResolver`]; unit tests inject a
/// fake that returns a fixed result table.
#[async_trait]
pub trait HostnameResolver: std::fmt::Debug + Send + Sync + 'static {
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
///
/// `last_refresh` is a monotonic [`tokio::time::Instant`] so cadence
/// comparisons are immune to wall-clock jumps (NTP step corrections,
/// manual clock changes, suspend-resume drift). `None` is the explicit
/// "refresh ASAP" sentinel set by [`HostnameRegistry::mark_stale`]
/// after a dial failure or an initial-empty resolve; cadence-elapsed
/// re-resolution uses `Some(t)`.
///
/// `stale_reason` is `Some(StaleReason::*)` exactly when `last_refresh`
/// is `None` and the zero-out came from `mark_stale`; the next
/// `pending_refreshes` tick reads it to decide whether the entry
/// belongs in the `InitialRetry` or `DialFailure` metric bucket. A
/// successful resolve clears `stale_reason` back to `None` along with
/// stamping `last_refresh = Some(now)`.
#[derive(Clone, Debug)]
pub struct HostnameRequest {
    pub host: Arc<str>,
    pub port: u16,
    pub is_permanent: bool,
    pub last_resolved: HashSet<SocketAddr>,
    pub last_refresh: Option<Instant>,
    pub stale_reason: Option<StaleReason>,
    pub refresh_failures: u32,
}

impl HostnameRequest {
    fn new(host: Arc<str>, port: u16, is_permanent: bool, initial: HashSet<SocketAddr>) -> Self {
        Self {
            host,
            port,
            is_permanent,
            last_resolved: initial,
            last_refresh: Some(Instant::now()),
            stale_reason: None,
            refresh_failures: 0,
        }
    }
}

/// Per-host outcome produced by a parallel resolve phase, consumed by
/// [`HostnameRegistry::apply_refresh_results`]. The `Option<Instant>`
/// is the snapshot value of `HostnameRequest.last_refresh` captured at
/// the moment `pending_refreshes` saw the entry; `apply_refresh_results`
/// compares it to the current value to detect a concurrent `mark_stale`
/// that fired between the snapshot and the apply (the race that would
/// otherwise let an `Ok` resolve clobber a `mark_stale`-set `None`).
pub type HostnameResolveOutcome = (Arc<str>, Option<Instant>, Result<Vec<SocketAddr>, PeerEndpointResolveError>);

/// Snapshot returned by [`HostnameRegistry::pending_refreshes`]. Carries
/// the per-entry [`ResolveTrigger`] derived from the eligibility reason
/// so a wakeup arm that aggregates both cadence-elapsed and mark-stale
/// entries never misattributes the metric label.
///
/// `snapshot_last_refresh` carries the entry's `last_refresh` value at
/// snapshot time; `apply_refresh_results` uses it as the load-bearing
/// race-detection signal: if the entry's current `last_refresh` differs
/// at apply time, a `mark_stale` fired during the resolve window, and
/// the apply preserves the `mark_stale`-set `None` rather than stamping
/// over it with `Some(now)`.
#[derive(Clone, Debug)]
pub struct PendingRefresh {
    pub host: Arc<str>,
    pub port: u16,
    pub trigger: ResolveTrigger,
    pub snapshot_last_refresh: Option<Instant>,
}

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

    /// Force the next refresh cadence check to treat `host` as eligible
    /// regardless of when it was last refreshed: clears `last_refresh`
    /// to `None` and records `reason` so
    /// [`HostnameRegistry::pending_refreshes`] yields the entry on the
    /// next tick with the matching [`ResolveTrigger`]
    /// (`StaleReason::InitialRetry` -> `ResolveTrigger::InitialRetry`,
    /// `StaleReason::DialFailure` -> `ResolveTrigger::DialFailure`).
    /// No-op if `host` is unknown.
    ///
    /// Race-detection contract: callers may invoke `mark_stale`
    /// repeatedly during a single resolve window -- each call
    /// overwrites `stale_reason` with the latest reason
    /// (last-write-wins). The
    /// [`HostnameRegistry::apply_refresh_results`] race-check only
    /// compares `last_refresh`, so the `stale_reason` field tracks the
    /// most recent intent at the moment apply runs. In the common case
    /// where the in-flight resolve is `Ok`, the latest `stale_reason`
    /// is cleared because the fresh resolve already satisfies the
    /// re-resolve intent; if the resolve was `Err`, the latest
    /// `stale_reason` remains and drives the next eligibility tick.
    pub fn mark_stale(&mut self, host: &str, reason: StaleReason) {
        if let Some(entry) = self.requests.get_mut(host) {
            entry.last_refresh = None;
            entry.stale_reason = Some(reason);
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

    /// Snapshot the hosts eligible for re-resolution on the next refresh
    /// tick, paired with the [`ResolveTrigger`] derived from each entry's
    /// eligibility reason:
    ///
    /// - `last_refresh = None` and `stale_reason = Some(InitialRetry)`
    ///   (set by `add_endpoint_request` for an entry whose initial
    ///   resolve was empty / errored) -> eligible, labeled
    ///   [`ResolveTrigger::InitialRetry`].
    /// - `last_refresh = None` and `stale_reason = Some(DialFailure)`
    ///   (set by `handle_connection_requests` after a dial loss against
    ///   a previously-resolved socket) -> eligible, labeled
    ///   [`ResolveTrigger::DialFailure`].
    /// - `last_refresh = None` and `stale_reason = Some(PeriodicEmpty)`
    ///   (set by `apply_refresh_results` / `refresh_all` after a
    ///   periodic resolve returned `Ok(empty)`) -> eligible, labeled
    ///   [`ResolveTrigger::Periodic`]. The metric label remains
    ///   `Periodic` because the next tick IS a periodic re-resolution
    ///   -- it is just accelerated past the cadence wait by the empty
    ///   outcome that established the stale flag.
    /// - `last_refresh = None` and `stale_reason = None` -> registry
    ///   invariant violation (every `mark_stale` stamps `stale_reason`
    ///   atomically with zeroing `last_refresh`). The entry is
    ///   ineligible; `pending_refreshes` skips it and the
    ///   `debug_assert` below trips in test/debug builds.
    /// - `last_refresh = Some(t)` and `now - t >= cadence` -> eligible,
    ///   labeled [`ResolveTrigger::Periodic`].
    /// - `last_refresh = Some(t)` and `now - t < cadence` -> not
    ///   eligible on this tick.
    ///
    /// The per-entry trigger is the fix for `peer_hostname_resolutions_total`
    /// label misattribution: a wakeup arm that aggregates cadence-elapsed
    /// entries together with mark-stale entries (which the dial-failure
    /// channel and the periodic ticker both legitimately do) records each
    /// entry under its own eligibility reason, not under the wakeup arm's
    /// trigger.
    ///
    /// Returned out-of-lock so the caller can run DNS lookups in parallel
    /// without holding the registry mutex across `await` points.
    pub fn pending_refreshes(&self, cadence: Duration) -> Vec<PendingRefresh> {
        let now = Instant::now();
        self.requests
            .iter()
            .filter_map(|(host, req)| {
                // Registry invariant: an entry whose `last_refresh` is
                // `None` always has `stale_reason = Some(_)` because
                // `mark_stale` always sets both fields together, and an
                // entry's `last_refresh` is only zeroed via `mark_stale`.
                // The (None, None) branch below is therefore unreachable
                // in production; in test/debug builds the `debug_assert`
                // turns any future regression into a fail-fast.
                debug_assert!(
                    !(req.last_refresh.is_none() && req.stale_reason.is_none()),
                    "registry invariant violation: entry {host:?} has last_refresh = None but stale_reason = None",
                );
                debug_assert!(
                    !(req.last_refresh.is_some() && req.stale_reason.is_some()),
                    "registry invariant violation: entry {host:?} has both last_refresh = Some and stale_reason = Some; \
                     the pair is written atomically by HostnameRequest::new, mark_stale, apply_refresh_results, refresh_all",
                );
                let trigger = match (req.last_refresh, req.stale_reason) {
                    (None, Some(StaleReason::InitialRetry)) => Some(ResolveTrigger::InitialRetry),
                    (None, Some(StaleReason::DialFailure)) => Some(ResolveTrigger::DialFailure),
                    (None, Some(StaleReason::PeriodicEmpty)) => Some(ResolveTrigger::Periodic),
                    (Some(t), _) if now.saturating_duration_since(t) >= cadence => Some(ResolveTrigger::Periodic),
                    (Some(_), _) => None,
                    // Unreachable per the invariant above; the
                    // `debug_assert!` on `pending_refreshes` re-entry
                    // guards this in test/debug builds.
                    (None, None) => None,
                };
                trigger.map(|tr| PendingRefresh {
                    host: host.clone(),
                    port: req.port,
                    trigger: tr,
                    snapshot_last_refresh: req.last_refresh,
                })
            })
            .collect()
    }

    /// Apply DNS results from a parallel resolve phase back into the
    /// registry. Called under the registry lock after the caller has
    /// run the resolves outside the lock. Bumps `refresh_failures` on
    /// `Err`; on `Ok` replaces `last_resolved` and resets
    /// `refresh_failures`. The cadence-anchor / stale-flag fields are
    /// updated according to the (resolved-emptiness, race-check)
    /// outcome described below. Returns one [`HostnameDelta`] per host
    /// with non-empty added/removed sets.
    ///
    /// `Ok(non-empty)`, no concurrent `mark_stale`: advance the cadence
    /// anchor (`last_refresh = Some(now)`) and clear the stale flag.
    ///
    /// `Ok(empty)`, no concurrent `mark_stale`: mark stale with
    /// [`StaleReason::PeriodicEmpty`] so the next eligibility tick
    /// re-resolves immediately (parity with the
    /// `ConnectionManager::add_endpoint_request` initial-empty fast-
    /// retry path). The `Ok(empty)` outcome is operationally equivalent
    /// to a failure-shape outcome -- the entry has zero resolved IPs
    /// and needs another DNS round to recover. The metric recording
    /// for this outcome happens in the caller (Phase 3a of
    /// `ConnectionManager::refresh_hostnames`) where `Ok(empty)` is
    /// folded into the `(<trigger>, Failed)` bucket -- mirroring
    /// `add_endpoint_request`'s `(Initial, Failed)` aggregation and
    /// rooted in Bitcoin Core's `ThreadOpenAddedConnections` /
    /// `getaddrinfo` parity (the upstream path does not distinguish
    /// "lookup returned no addresses" from "lookup errored / timed out"
    /// either).
    ///
    /// `Ok(_)`, concurrent `mark_stale` fired during the resolve
    /// window (`entry.last_refresh != snapshot_last_refresh`): preserve
    /// the `mark_stale`-set state regardless of the resolve outcome.
    /// Race-detection covers the headline `(Some(t), None)` interleaving
    /// where a fresh `mark_stale` would otherwise be silently overwritten
    /// by the in-flight `Ok` result, and equally covers the symmetric
    /// case where an in-flight `Ok(empty)` would otherwise overwrite a
    /// concurrent `DialFailure` with `PeriodicEmpty`.
    ///
    /// `last_resolved` and `refresh_failures` are updated regardless
    /// of the equality outcome on the `Ok` arm -- `last_resolved` is
    /// replaced with the freshly-resolved set (whether empty or not),
    /// and `refresh_failures` is reset to zero. Only `last_refresh`
    /// and `stale_reason` are gated by the race-check + emptiness
    /// classification.
    ///
    /// **Metric-attribution caveat in the `(None, None)` interleaving.**
    /// When the snapshot was already `None` and a SECOND `mark_stale`
    /// (e.g. `DialFailure` overwriting `InitialRetry`) fires during a
    /// successful in-flight resolve, the equality check
    /// `entry.last_refresh == snapshot_last_refresh` still holds (both
    /// values are `None`) and the apply takes the non-race arm: on
    /// `Ok(non-empty)` it stamps `last_refresh = Some(now)` and clears
    /// `stale_reason`. The second `mark_stale`-set reason is not
    /// preserved -- the in-flight `Ok` already produced fresh IPs that
    /// satisfy the re-resolve intent. Operators monitoring
    /// `dial_failure_*` will see the next eligibility increment land
    /// under `periodic_*` until a fresh dial failure re-fires
    /// `mark_stale(DialFailure)` on the next dial tick. Brief metric
    /// attribution skew (one cadence tick) is the cost; correctness
    /// holds (the entry recovers within bounded time, no sockets are
    /// lost or stranded). Tightening this would require a second
    /// equality predicate on `stale_reason`, which the same-value-
    /// equals-equal contract on `last_refresh` does not provide.
    pub fn apply_refresh_results(&mut self, results: Vec<HostnameResolveOutcome>) -> Vec<HostnameDelta> {
        let now = Instant::now();
        let mut deltas = Vec::with_capacity(results.len());
        for (host, snapshot_last_refresh, result) in results {
            let Some(entry) = self.requests.get_mut(&host) else { continue };
            match result {
                Ok(resolved) => {
                    let new_set: HashSet<SocketAddr> = resolved.into_iter().collect();
                    let new_set_empty = new_set.is_empty();
                    let prev = std::mem::take(&mut entry.last_resolved);
                    let added: Vec<SocketAddr> = new_set.difference(&prev).copied().collect();
                    let removed: Vec<SocketAddr> = prev.difference(&new_set).copied().collect();
                    entry.last_resolved = new_set;
                    entry.refresh_failures = 0;
                    match (new_set_empty, entry.last_refresh == snapshot_last_refresh) {
                        (false, true) => {
                            // Ok(non-empty), no race: advance cadence anchor.
                            entry.last_refresh = Some(now);
                            entry.stale_reason = None;
                        }
                        (true, true) => {
                            // Ok(empty), no race: mark PeriodicEmpty for fast retry.
                            entry.last_refresh = None;
                            entry.stale_reason = Some(StaleReason::PeriodicEmpty);
                        }
                        (_, false) => {
                            // Concurrent mark_stale fired during the resolve
                            // window: preserve the mark_stale-set state.
                        }
                    }
                    if !added.is_empty() || !removed.is_empty() {
                        deltas.push(HostnameDelta { host, added, removed });
                    }
                }
                Err(_e) => {
                    entry.refresh_failures = entry.refresh_failures.saturating_add(1);
                }
            }
        }
        deltas
    }

    /// Resolve every hostname entry through `resolver`, record the result
    /// in `metrics` under the given `trigger` label, and return the
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
    pub async fn refresh_all<R: HostnameResolver + ?Sized>(
        &mut self,
        resolver: &R,
        metrics: &HostnameMetrics,
        trigger: ResolveTrigger,
    ) -> Vec<HostnameDelta> {
        let snapshots: Vec<(Arc<str>, u16, HashSet<SocketAddr>)> =
            self.requests.iter().map(|(k, v)| (k.clone(), v.port, v.last_resolved.clone())).collect();
        let mut deltas = Vec::with_capacity(snapshots.len());
        for (host, port, prev) in snapshots {
            match resolver.resolve(&host, port).await {
                Ok(resolved) => {
                    let new_set: HashSet<SocketAddr> = resolved.into_iter().collect();
                    let new_set_empty = new_set.is_empty();
                    // `Ok(empty)` folds into the `(<trigger>, Failed)`
                    // metric bucket: the resolver returned zero IPs, the
                    // entry has nothing to dial, and the fast-retry path
                    // engages via `StaleReason::PeriodicEmpty` below --
                    // mirroring the `add_endpoint_request` initial-empty
                    // semantics across every trigger label.
                    let status = if new_set_empty { ResolveStatus::Failed } else { ResolveStatus::Ok };
                    metrics.record(trigger, status);
                    let added: Vec<SocketAddr> = new_set.difference(&prev).copied().collect();
                    let removed: Vec<SocketAddr> = prev.difference(&new_set).copied().collect();
                    if let Some(entry) = self.requests.get_mut(&host) {
                        entry.last_resolved = new_set;
                        if new_set_empty {
                            entry.last_refresh = None;
                            entry.stale_reason = Some(StaleReason::PeriodicEmpty);
                        } else {
                            entry.last_refresh = Some(Instant::now());
                            entry.stale_reason = None;
                        }
                        entry.refresh_failures = 0;
                    }
                    if !added.is_empty() || !removed.is_empty() {
                        deltas.push(HostnameDelta { host: host.clone(), added, removed });
                    }
                }
                Err(_e) => {
                    metrics.record(trigger, ResolveStatus::Failed);
                    if let Some(entry) = self.requests.get_mut(&host) {
                        entry.refresh_failures = entry.refresh_failures.saturating_add(1);
                    }
                }
            }
        }
        deltas
    }

    /// Sum of distinct resolved socket addresses across all hostname
    /// entries (gauge value for `peer_hostname_resolved_addrs`).
    pub fn total_resolved_addrs(&self) -> usize {
        self.requests.values().map(|r| r.last_resolved.len()).sum()
    }

    /// Return the snapshot of `(host, last_resolved)` pairs needed by the
    /// dial-failure handler to decide which hostname owns a given socket.
    /// Test-only: the production back-reference uses
    /// `ConnectionRequest.hostname_origin` directly; this method exists
    /// for the registry-side authoritative inverse and is exercised only
    /// by unit tests of the registry's reverse map.
    #[cfg(test)]
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
    #[derive(Debug)]
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
        let deltas = reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
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
        let deltas = reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
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
        let deltas = reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
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
        let deltas = reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
        assert!(deltas.is_empty(), "expected empty delta for unchanged DNS, got {deltas:?}");
        let deltas = reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
        assert!(deltas.is_empty());
    }

    #[tokio::test]
    async fn registry_refresh_failure_does_not_drop_entry() {
        let mut reg = HostnameRegistry::new();
        let prev: HashSet<SocketAddr> = [sock("10.0.0.1:16111")].into_iter().collect();
        reg.upsert("a.example", 16111, true, prev);
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Err("synthetic DNS failure".to_owned()));
        let deltas = reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
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
            reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
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
        let deltas = reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].added.len(), 3);
        assert!(deltas[0].removed.is_empty());
    }

    #[tokio::test]
    async fn registry_mark_stale_clears_last_refresh() {
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, [sock("10.0.0.1:16111")].into_iter().collect());
        let before = reg.get("a.example").unwrap().last_refresh;
        reg.mark_stale("a.example", StaleReason::DialFailure);
        let after = reg.get("a.example").unwrap().last_refresh;
        assert!(before.is_some(), "fresh upsert must record last_refresh");
        assert!(after.is_none(), "mark_stale must clear last_refresh to the refresh-ASAP sentinel");
        // Marking an unknown host is a no-op.
        reg.mark_stale("not.in.registry", StaleReason::DialFailure);
    }

    #[tokio::test]
    async fn registry_pending_refreshes_labels_per_entry_trigger() {
        // Cadence-elapsed entries are eligible as Periodic; mark-stale
        // entries are eligible as DialFailure regardless of cadence.
        let mut reg = HostnameRegistry::new();
        reg.upsert("periodic.example", 16111, true, HashSet::new());
        reg.upsert("stale.example", 16111, true, HashSet::new());
        reg.mark_stale("stale.example", StaleReason::DialFailure);
        reg.upsert("retry.example", 16111, true, HashSet::new());
        reg.mark_stale("retry.example", StaleReason::InitialRetry);

        // Force the periodic entry to have an elapsed cadence by clearing
        // and re-stamping `last_refresh` to a moment far enough in the
        // past for any plausible `cadence`. We cannot easily backdate
        // `Instant`, so use `Duration::ZERO` as the cadence -- every
        // `Some(_)` entry then qualifies as periodic-elapsed.
        let pending = reg.pending_refreshes(Duration::ZERO);
        let by_host: StdHashMap<&str, ResolveTrigger> = pending.iter().map(|p| (p.host.as_ref(), p.trigger)).collect();
        assert_eq!(by_host.get("periodic.example").copied(), Some(ResolveTrigger::Periodic));
        assert_eq!(by_host.get("stale.example").copied(), Some(ResolveTrigger::DialFailure));
        assert_eq!(by_host.get("retry.example").copied(), Some(ResolveTrigger::InitialRetry));
    }

    #[tokio::test]
    async fn registry_pending_refreshes_skips_when_cadence_unmet() {
        // Fresh upsert, large cadence -> nothing eligible (no mark_stale).
        let mut reg = HostnameRegistry::new();
        reg.upsert("recent.example", 16111, true, HashSet::new());
        let pending = reg.pending_refreshes(Duration::from_secs(3600));
        assert!(pending.is_empty(), "fresh entry must not be eligible until cadence elapses");
        // Mark stale -> eligible immediately as DialFailure regardless of cadence.
        reg.mark_stale("recent.example", StaleReason::DialFailure);
        let pending = reg.pending_refreshes(Duration::from_secs(3600));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].trigger, ResolveTrigger::DialFailure);
    }

    #[tokio::test]
    async fn registry_apply_preserves_concurrent_mark_stale() {
        // Race-detection contract: when `apply_refresh_results` runs after
        // a concurrent `mark_stale` flipped `last_refresh` from `Some(_)`
        // to `None`, the apply must NOT stamp `last_refresh = Some(now)`
        // or clear `stale_reason` -- the `mark_stale`-set trigger drives
        // the next eligibility tick. `last_resolved` and `refresh_failures`
        // ARE updated regardless (the in-flight `Ok` already produced
        // fresh IPs that satisfy the re-resolve intent).
        let mut reg = HostnameRegistry::new();
        let initial = sock("10.0.0.1:16111");
        reg.upsert("a.example", 16111, true, [initial].into_iter().collect());
        // Snapshot `last_refresh` as a parallel resolve would have
        // captured it via `pending_refreshes` -> `PendingRefresh.snapshot_last_refresh`.
        let snapshot = reg.get("a.example").unwrap().last_refresh;
        assert!(snapshot.is_some(), "fresh upsert must have last_refresh = Some(_)");
        // Concurrent mark_stale fires while the in-flight resolve is running.
        reg.mark_stale("a.example", StaleReason::DialFailure);
        // Apply the in-flight Ok with the snapshot captured before mark_stale.
        let fresh = sock("10.0.0.2:16111");
        let host = reg.get("a.example").unwrap().host.clone();
        reg.apply_refresh_results(vec![(host, snapshot, Ok(vec![fresh]))]);
        let entry = reg.get("a.example").unwrap();
        assert!(entry.last_refresh.is_none(), "apply must preserve mark_stale-set None last_refresh");
        assert_eq!(entry.stale_reason, Some(StaleReason::DialFailure), "apply must preserve mark_stale-set stale_reason");
        // Always-updated fields: fresh IPs land, refresh_failures resets.
        assert!(entry.last_resolved.contains(&fresh), "Ok arm must update last_resolved with fresh IPs");
        assert!(!entry.last_resolved.contains(&initial), "Ok arm replaces last_resolved (does not merge)");
        assert_eq!(entry.refresh_failures, 0);
    }

    #[tokio::test]
    async fn registry_apply_advances_anchor_when_no_concurrent_mark_stale() {
        // Sibling case: when the snapshot equals the current `last_refresh`,
        // no concurrent `mark_stale` fired -- the apply stamps `last_refresh`
        // and clears `stale_reason` as the normal periodic-refresh outcome.
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, HashSet::new());
        let snapshot = reg.get("a.example").unwrap().last_refresh;
        let fresh = sock("10.0.0.1:16111");
        let host = reg.get("a.example").unwrap().host.clone();
        reg.apply_refresh_results(vec![(host, snapshot, Ok(vec![fresh]))]);
        let entry = reg.get("a.example").unwrap();
        assert!(entry.last_refresh.is_some(), "apply must stamp last_refresh = Some(now)");
        assert_ne!(entry.last_refresh, snapshot, "apply must replace the snapshot anchor");
        assert_eq!(entry.stale_reason, None, "apply must clear stale_reason when no race");
    }

    #[tokio::test]
    async fn registry_apply_benign_double_stale_interleaving() {
        // (None, _) -> (None, _): entry is already stale at snapshot time,
        // a second `mark_stale` fires during the resolve, and both
        // `last_refresh` values are `None` so the equality check passes.
        // The in-flight `Ok` then satisfies the re-resolve intent: the
        // apply stamps `last_refresh = Some(now)` and clears
        // `stale_reason` (bounded recovery within one cadence; if the
        // new IPs subsequently fail to dial, `mark_stale(DialFailure)`
        // re-fires on the next dial tick).
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, HashSet::new());
        reg.mark_stale("a.example", StaleReason::InitialRetry);
        let snapshot = reg.get("a.example").unwrap().last_refresh;
        assert!(snapshot.is_none(), "mark_stale must zero last_refresh");
        // Second mark_stale during the resolve -- last-write-wins on stale_reason.
        reg.mark_stale("a.example", StaleReason::DialFailure);
        let fresh = sock("10.0.0.1:16111");
        let host = reg.get("a.example").unwrap().host.clone();
        reg.apply_refresh_results(vec![(host, snapshot, Ok(vec![fresh]))]);
        let entry = reg.get("a.example").unwrap();
        assert!(entry.last_refresh.is_some(), "(None, _) -> (None, _) interleaving stamps last_refresh");
        assert_eq!(entry.stale_reason, None, "(None, _) -> (None, _) interleaving clears stale_reason");
        assert!(entry.last_resolved.contains(&fresh));
    }

    #[tokio::test]
    async fn registry_apply_periodic_empty_marks_stale_when_no_concurrent_mark_stale() {
        // `Ok(empty)` on a previously-resolved entry, no concurrent
        // `mark_stale`: the apply MUST mark stale with PeriodicEmpty so
        // the next eligibility tick re-resolves immediately (parity with
        // the `add_endpoint_request` initial-empty fast-retry path). The
        // empty outcome is operationally a failure-shape result and
        // belongs on the fast-retry side of the cadence wait.
        let mut reg = HostnameRegistry::new();
        let initial = sock("10.0.0.1:16111");
        reg.upsert("a.example", 16111, true, [initial].into_iter().collect());
        let snapshot = reg.get("a.example").unwrap().last_refresh;
        assert!(snapshot.is_some(), "fresh upsert must have last_refresh = Some(_)");
        let host = reg.get("a.example").unwrap().host.clone();
        // Apply Ok(empty) with the matching snapshot (no concurrent mark_stale).
        let deltas = reg.apply_refresh_results(vec![(host, snapshot, Ok(Vec::new()))]);
        let entry = reg.get("a.example").unwrap();
        assert!(entry.last_refresh.is_none(), "Ok(empty) with no race must zero last_refresh");
        assert_eq!(
            entry.stale_reason,
            Some(StaleReason::PeriodicEmpty),
            "Ok(empty) with no race must mark PeriodicEmpty for fast retry",
        );
        assert!(entry.last_resolved.is_empty(), "Ok(empty) replaces last_resolved with the empty set");
        assert_eq!(entry.refresh_failures, 0, "refresh_failures resets on the Ok arm regardless of emptiness");
        assert_eq!(deltas.len(), 1, "delta produced for removed initial IP");
        assert_eq!(deltas[0].removed, vec![initial]);
        assert!(deltas[0].added.is_empty());
    }

    #[tokio::test]
    async fn registry_apply_periodic_empty_preserves_concurrent_mark_stale() {
        // `Ok(empty)` arrives but a concurrent `mark_stale(DialFailure)`
        // fired during the resolve window. The race-detection equality
        // check fires (snapshot != current) and the apply preserves the
        // mark_stale-set DialFailure intent rather than overwriting with
        // PeriodicEmpty. `last_resolved` and `refresh_failures` still
        // update unconditionally on the Ok arm.
        let mut reg = HostnameRegistry::new();
        let initial = sock("10.0.0.1:16111");
        reg.upsert("a.example", 16111, true, [initial].into_iter().collect());
        let snapshot = reg.get("a.example").unwrap().last_refresh;
        assert!(snapshot.is_some());
        // Concurrent mark_stale fires during the resolve.
        reg.mark_stale("a.example", StaleReason::DialFailure);
        let host = reg.get("a.example").unwrap().host.clone();
        // Apply Ok(empty) with the captured-pre-stale snapshot.
        reg.apply_refresh_results(vec![(host, snapshot, Ok(Vec::new()))]);
        let entry = reg.get("a.example").unwrap();
        assert!(entry.last_refresh.is_none(), "concurrent mark_stale already zeroed last_refresh");
        assert_eq!(
            entry.stale_reason,
            Some(StaleReason::DialFailure),
            "concurrent mark_stale-set DialFailure must NOT be overwritten by PeriodicEmpty",
        );
        assert!(entry.last_resolved.is_empty(), "Ok arm always replaces last_resolved with the new set");
        assert_eq!(entry.refresh_failures, 0);
    }

    #[tokio::test]
    async fn registry_pending_refreshes_periodic_empty_eligible_as_periodic() {
        // An entry whose `stale_reason` is PeriodicEmpty is eligible on
        // the next tick regardless of cadence, and the per-entry
        // metric label is Periodic (the next tick IS a periodic
        // re-resolution).
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, HashSet::new());
        // Drive into the PeriodicEmpty state via apply_refresh_results.
        let snapshot = reg.get("a.example").unwrap().last_refresh;
        let host = reg.get("a.example").unwrap().host.clone();
        reg.apply_refresh_results(vec![(host, snapshot, Ok(Vec::new()))]);
        assert_eq!(reg.get("a.example").unwrap().stale_reason, Some(StaleReason::PeriodicEmpty));
        // Now even a 1-hour cadence yields the entry immediately.
        let pending = reg.pending_refreshes(Duration::from_secs(3600));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].trigger, ResolveTrigger::Periodic, "PeriodicEmpty stale_reason must label as Periodic");
        assert!(pending[0].snapshot_last_refresh.is_none(), "snapshot_last_refresh forwarded as None for stale entries");
    }

    #[tokio::test]
    async fn registry_refresh_all_empty_marks_periodic_empty_and_records_failed() {
        // `refresh_all` Ok(empty) outcome: metric records (trigger,
        // Failed) and the entry transitions to (None, Some(PeriodicEmpty))
        // -- mirroring apply_refresh_results semantics so the legacy
        // unit-test ergonomic path stays aligned with the production
        // pending_refreshes / apply_refresh_results pair.
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, [sock("10.0.0.1:16111")].into_iter().collect());
        let metrics = HostnameMetrics::default();
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Ok(Vec::new()));
        reg.refresh_all(&resolver, &metrics, ResolveTrigger::Periodic).await;
        let entry = reg.get("a.example").unwrap();
        assert!(entry.last_refresh.is_none(), "Ok(empty) zeros last_refresh under refresh_all");
        assert_eq!(entry.stale_reason, Some(StaleReason::PeriodicEmpty));
        assert!(entry.last_resolved.is_empty());
        assert_eq!(entry.refresh_failures, 0);
        let snap = metrics.snapshot();
        assert_eq!(snap.resolutions_total.periodic_failed, 1, "Ok(empty) increments (<trigger>, Failed)");
        assert_eq!(snap.resolutions_total.periodic_ok, 0);
    }

    #[tokio::test]
    async fn registry_apply_preserves_rival_stamp_under_none_snapshot() {
        // 4-quadrant matrix completion: snapshot = None, current = Some(_).
        // One `pending_refreshes` cycle captures a stale entry with
        // `last_refresh = None` (the snapshot), runs a resolve. Meanwhile
        // a SECOND cycle has already stamped `last_refresh = Some(now)`
        // (the rival stamp). The first cycle's apply then sees
        // `snapshot = None != current = Some(_)` and the equality check
        // (correctly) preserves the more recent rival stamp.
        // Always-updated fields (`last_resolved`, `refresh_failures`)
        // still apply because the in-flight `Ok` satisfies the
        // re-resolve intent.
        let mut reg = HostnameRegistry::new();
        let initial = sock("10.0.0.1:16111");
        reg.upsert("a.example", 16111, true, [initial].into_iter().collect());
        // Move to "stale" state -> capture snapshot = None.
        reg.mark_stale("a.example", StaleReason::InitialRetry);
        let snapshot = reg.get("a.example").unwrap().last_refresh;
        assert!(snapshot.is_none(), "snapshot must be None for this quadrant");
        // Rival cycle completes first: stamps current = Some(now), clears stale_reason.
        let rival_ip = sock("10.0.0.2:16111");
        let host = reg.get("a.example").unwrap().host.clone();
        reg.apply_refresh_results(vec![(host.clone(), snapshot, Ok(vec![rival_ip]))]);
        let rival_stamp = reg.get("a.example").unwrap().last_refresh;
        assert!(rival_stamp.is_some(), "rival apply must have stamped last_refresh");
        // Now the in-flight first-cycle apply lands with snapshot = None
        // but current = rival_stamp = Some(_).
        let late_fresh = sock("10.0.0.3:16111");
        reg.apply_refresh_results(vec![(host, snapshot, Ok(vec![late_fresh]))]);
        let entry = reg.get("a.example").unwrap();
        assert_eq!(entry.last_refresh, rival_stamp, "rival stamp preserved (snapshot != current)");
        assert_eq!(entry.stale_reason, None, "rival apply already cleared stale_reason; second apply does not flip it");
        // Always-updated: late-cycle Ok still installs its IPs and resets refresh_failures.
        assert!(entry.last_resolved.contains(&late_fresh));
        assert!(!entry.last_resolved.contains(&initial), "Ok arm replaces last_resolved (does not merge)");
        assert_eq!(entry.refresh_failures, 0);
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
        reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
        assert_eq!(reg.get("a.example").unwrap().refresh_failures, 1);
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.1:16111")]));
        reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
        assert_eq!(reg.get("a.example").unwrap().refresh_failures, 0);
    }

    #[tokio::test]
    async fn metrics_record_initial_ok_and_failed_separately() {
        let metrics = HostnameMetrics::default();
        metrics.record(ResolveTrigger::Initial, ResolveStatus::Ok);
        metrics.record(ResolveTrigger::Initial, ResolveStatus::Failed);
        metrics.record(ResolveTrigger::Initial, ResolveStatus::Ok);
        assert_eq!(metrics.get(ResolveTrigger::Initial, ResolveStatus::Ok), 2);
        assert_eq!(metrics.get(ResolveTrigger::Initial, ResolveStatus::Failed), 1);
        assert_eq!(metrics.get(ResolveTrigger::Periodic, ResolveStatus::Ok), 0);
        assert_eq!(metrics.get(ResolveTrigger::DialFailure, ResolveStatus::Failed), 0);
    }

    #[tokio::test]
    async fn metrics_record_refresh_periodic_then_dial_failure_buckets() {
        let mut reg = HostnameRegistry::new();
        reg.upsert("a.example", 16111, true, HashSet::new());
        let metrics = HostnameMetrics::default();
        let resolver = FakeResolver::new();
        resolver.set("a.example", 16111, Ok(vec![sock("10.0.0.1:16111")]));
        reg.refresh_all(&resolver, &metrics, ResolveTrigger::Periodic).await;
        resolver.set("a.example", 16111, Err("synthetic".to_owned()));
        reg.refresh_all(&resolver, &metrics, ResolveTrigger::DialFailure).await;
        let snap = metrics.snapshot();
        assert_eq!(snap.resolutions_total.periodic_ok, 1);
        assert_eq!(snap.resolutions_total.dial_failure_failed, 1);
        assert_eq!(snap.resolutions_total.dial_failure_ok, 0);
        assert_eq!(snap.resolutions_total.periodic_failed, 0);
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
        reg.refresh_all(&resolver, &HostnameMetrics::default(), ResolveTrigger::Periodic).await;
        assert_eq!(reg.get("ok.example").unwrap().refresh_failures, 0);
        assert_eq!(reg.get("ok.example").unwrap().last_resolved.len(), 1);
        assert_eq!(reg.get("bad.example").unwrap().refresh_failures, 1);
        assert!(reg.get("bad.example").unwrap().last_resolved.is_empty());
    }
}
