use crate::hasher::KaspaDiff;
use kaspa_consensus_core::block::Block;
use kaspa_hashes::Hash;
use num_bigint::BigUint;
use num_traits::Zero;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tracing;

const MAX_JOBS: u64 = 300;

/// Job structure that holds both the block and the pre-PoW hash
/// The pre-PoW hash is what we send to the ASIC for mining
#[derive(Debug, Clone)]
pub struct Job {
    pub block: Block,
    pub pre_pow_hash: Hash,
}

/// Mining state for a client connection
#[derive(Debug)]
pub struct MiningState {
    jobs: Arc<Mutex<HashMap<u64, Job>>>,
    job_ids: Arc<Mutex<HashMap<u64, u64>>>, // Maps slot index to actual job ID
    job_counter: Arc<Mutex<u64>>,
    big_diff: Arc<Mutex<BigUint>>,
    initialized: Arc<Mutex<bool>>,
    use_big_job: Arc<Mutex<bool>>,
    connect_time: SystemTime,
    stratum_diff: Arc<Mutex<Option<KaspaDiff>>>,
    max_jobs: u16,
    last_header: Arc<Mutex<Option<kaspa_consensus_core::header::Header>>>, // Track previous header for change logging
}

impl MiningState {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            job_ids: Arc::new(Mutex::new(HashMap::new())),
            job_counter: Arc::new(Mutex::new(0)),
            big_diff: Arc::new(Mutex::new(BigUint::zero())),
            initialized: Arc::new(Mutex::new(false)),
            use_big_job: Arc::new(Mutex::new(false)),
            connect_time: SystemTime::now(),
            stratum_diff: Arc::new(Mutex::new(None)),
            max_jobs: MAX_JOBS as u16,
            last_header: Arc::new(Mutex::new(None)),
        }
    }

    /// Add a new job and return its ID
    pub fn add_job(&self, job: Job) -> u64 {
        let mut counter = self.job_counter.lock();
        *counter += 1;
        let idx = *counter;
        let slot = idx % MAX_JOBS;
        
        let mut jobs = self.jobs.lock();
        let mut job_ids = self.job_ids.lock();
        
        // Log if we're overwriting an old job
        if let Some(old_id) = job_ids.get(&slot) {
            tracing::debug!("Overwriting job at slot {}: old_id={}, new_id={}", slot, old_id, idx);
        }
        
        jobs.insert(slot, job);
        job_ids.insert(slot, idx);
        
        tracing::debug!("[JOB STORAGE] Added job ID {} at slot {} (counter now: {})", idx, slot, idx);
        idx
    }

    /// Get a job by ID
    /// Return job at slot (id % maxJobs) without verifying ID matches
    ///          return job, exists
    /// Does NOT verify that the stored job ID matches - it just returns whatever is at that slot
    pub fn get_job(&self, id: u64) -> Option<Job> {
        let jobs = self.jobs.lock();
        let slot = id % MAX_JOBS;
        
        // Return job at slot, don't verify ID matches
        jobs.get(&slot).cloned()
    }
    
    /// Get job ID at a specific slot (for debugging/stale job workaround)
    pub fn get_job_id_at_slot(&self, slot: u64) -> Option<u64> {
        let job_ids = self.job_ids.lock();
        job_ids.get(&(slot % MAX_JOBS)).copied()
    }

    /// Set the big difficulty (network target)
    pub fn set_big_diff(&self, diff: BigUint) {
        *self.big_diff.lock() = diff;
    }

    /// Get the big difficulty
    pub fn get_big_diff(&self) -> BigUint {
        self.big_diff.lock().clone()
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        *self.initialized.lock()
    }

    /// Set initialized
    pub fn set_initialized(&self, initialized: bool) {
        *self.initialized.lock() = initialized;
    }

    /// Check if using big job format
    pub fn use_big_job(&self) -> bool {
        *self.use_big_job.lock()
    }

    /// Set use big job format
    pub fn set_use_big_job(&self, use_big: bool) {
        *self.use_big_job.lock() = use_big;
    }

    /// Get connect time
    pub fn connect_time(&self) -> SystemTime {
        self.connect_time
    }

    /// Get stratum difficulty
    pub fn stratum_diff(&self) -> Option<KaspaDiff> {
        self.stratum_diff.lock().clone()
    }

    /// Set stratum difficulty
    pub fn set_stratum_diff(&self, diff: KaspaDiff) {
        *self.stratum_diff.lock() = Some(diff);
    }

    /// Get max jobs
    pub fn max_jobs(&self) -> u16 {
        self.max_jobs
    }

    /// Get current job counter (for debugging)
    pub fn current_job_counter(&self) -> u64 {
        *self.job_counter.lock()
    }

    /// Get stored job IDs (for debugging)
    pub fn get_stored_job_ids(&self) -> Vec<u64> {
        let job_ids = self.job_ids.lock();
        job_ids.values().copied().collect()
    }
    
    /// Get last header
    pub fn get_last_header(&self) -> Option<kaspa_consensus_core::header::Header> {
        self.last_header.lock().clone()
    }
    
    /// Set last header
    pub fn set_last_header(&self, header: kaspa_consensus_core::header::Header) {
        *self.last_header.lock() = Some(header);
    }
}

impl Default for MiningState {
    fn default() -> Self {
        Self::new()
    }
}

/// Get MiningState from StratumContext
#[allow(non_snake_case)]
pub fn GetMiningState(ctx: &crate::stratum_context::StratumContext) -> Arc<MiningState> {
    // State is now stored directly as Arc<MiningState>, so we can just clone it
    Arc::clone(&ctx.state)
}

