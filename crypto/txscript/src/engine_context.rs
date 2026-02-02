use crate::caches::Cache;
use crate::covenants::{CovenantsContext, EMPTY_COV_CONTEXT};
use crate::{SeqCommitAccessor, SigCacheKey};
use kaspa_consensus_core::hashing::sighash::{SigHashReusedValues, SigHashReusedValuesSync, SigHashReusedValuesUnsync};

/// Marker type indicating that sig-hash reused values have not been bound yet.
#[derive(Default)]
pub struct MissingReusedValues;

pub struct EngineContext<'a, Reused> {
    pub(crate) reused_values: &'a Reused,
    pub(crate) sig_cache: &'a SigCache,
    pub(crate) covenants_ctx: &'a CovenantsContext,
    pub(crate) seq_commit_accessor: Option<&'a dyn SeqCommitAccessor>,
}

impl<'a, Reused> EngineContext<'a, Reused> {
    /// Attach a covenants context to an existing engine context.
    #[inline]
    pub fn with_covenants_ctx(mut self, covenants_ctx: &'a CovenantsContext) -> Self {
        self.covenants_ctx = covenants_ctx;
        self
    }

    #[inline]
    pub fn with_seq_commit_accessor(mut self, seq_commit_accessor: &'a dyn SeqCommitAccessor) -> Self {
        self.seq_commit_accessor = Some(seq_commit_accessor);
        self
    }

    #[inline]
    pub fn with_seq_commit_accessor_opt(mut self, seq_commit_accessor: Option<&'a dyn SeqCommitAccessor>) -> Self {
        self.seq_commit_accessor = seq_commit_accessor;
        self
    }
}

type SigCache = Cache<SigCacheKey, bool>;

impl<'a> EngineContext<'a, MissingReusedValues> {
    const MISSING: MissingReusedValues = MissingReusedValues;

    /// Create a context without bound reused values, using an empty covenants context.
    pub fn new(sig_cache: &'a SigCache) -> Self {
        Self { reused_values: &Self::MISSING, sig_cache, covenants_ctx: &EMPTY_COV_CONTEXT, seq_commit_accessor: None }
    }

    /// Upgrade the context by binding concrete sig-hash reused values (one-way).
    #[inline]
    pub fn with_reused<New: SigHashReusedValues>(self, reused_values: &'a New) -> EngineContext<'a, New> {
        EngineContext {
            reused_values,
            sig_cache: self.sig_cache,
            covenants_ctx: self.covenants_ctx,
            seq_commit_accessor: self.seq_commit_accessor,
        }
    }
}

impl<'a, Reused: SigHashReusedValues> Clone for EngineContext<'a, Reused> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, Reused: SigHashReusedValues> Copy for EngineContext<'a, Reused> {}

/// Engine context before binding sig-hash reused values.
pub type EngineCtx<'a> = EngineContext<'a, MissingReusedValues>;

/// Engine context for parallel script checking (thread-safe reused values).
pub type EngineCtxSync<'a> = EngineContext<'a, SigHashReusedValuesSync>;

/// Engine context for sequential script checking (non-thread-safe reused values).
pub type EngineCtxUnsync<'a> = EngineContext<'a, SigHashReusedValuesUnsync>;
