//! Constants used throughout the RustBridge application
//!
//! This module centralizes magic numbers and timing values to improve maintainability
//! and make the codebase more self-documenting.

use std::time::Duration;

// ============================================================================
// Timing Constants
// ============================================================================

/// Delay before sending immediate job after authorization
/// Critical for IceRiver ASICs - they expect immediate job delivery
pub const IMMEDIATE_JOB_DELAY: Duration = Duration::from_millis(100);

/// Timeout for client to provide wallet address after connection
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(20);

/// Interval between balance checks for connected miners
pub const BALANCE_DELAY: Duration = Duration::from_secs(60);

/// Minimum time between sending block templates (rate limiting)
pub const BLOCK_TEMPLATE_RATE_LIMIT: Duration = Duration::from_millis(250);

/// Interval for pruning old statistics
pub const STATS_PRUNE_INTERVAL: Duration = Duration::from_secs(60);

/// Interval for printing statistics to console
pub const STATS_PRINT_INTERVAL: Duration = Duration::from_secs(10);

/// Delay before creating worker statistics (gives time for authorization)
pub const STATS_CREATION_DELAY: Duration = Duration::from_secs(5);

/// Interval between new block template checks
pub const BLOCK_WAIT_DEFAULT: Duration = Duration::from_millis(1000);

// ============================================================================
// Buffer and Size Constants
// ============================================================================

/// Size of read buffer for TCP connections
pub const READ_BUFFER_SIZE: usize = 1024;

/// Maximum number of jobs to store per client (circular buffer size)
pub const MAX_JOBS: u64 = 300;

/// Maximum number of jobs stored (used for job slot calculation)
pub const MAX_JOBS_U16: u16 = 300;

// ============================================================================
// Extranonce Constants
// ============================================================================

/// Extranonce size for IceRiver, BzMiner, and Goldshell miners (bytes)
pub const EXTRANONCE_SIZE_NON_BITMAIN: i8 = 2;

/// Extranonce size for Bitmain miners (bytes)
/// Bitmain doesn't use extranonce (extranonce_size = 0)
pub const EXTRANONCE_SIZE_BITMAIN: i8 = 0;

/// Maximum extranonce value for 2-byte extranonce (2^16 - 1 = 65535)
pub const MAX_EXTRANONCE_VALUE: i32 = 65535;

/// Expected extranonce2 size for Bitmain (8 bytes total - 0 extranonce = 8)
pub const BITMAIN_EXTRANONCE2_SIZE: i32 = 8;

// ============================================================================
// Miner Detection Keywords
// ============================================================================

/// Keywords used to detect Bitmain miners (case-insensitive matching)
/// Matches: "godminer", "bitmain", "antminer"
pub const BITMAIN_KEYWORDS: &[&str] = &["godminer", "bitmain", "antminer"];

/// Keywords used to detect IceRiver miners (case-insensitive matching)
/// Matches: "iceriver", "icemining", "icm"
pub const ICERIVER_KEYWORDS: &[&str] = &["iceriver", "icemining", "icm"];

// ============================================================================
// Retry and Timeout Constants
// ============================================================================

/// Maximum number of retries for block template fetching
pub const BLOCK_TEMPLATE_MAX_RETRIES: usize = 3;

/// Base delay multiplier for retries (multiplied by attempt number)
pub const RETRY_DELAY_BASE_MS: u64 = 100;

/// Write timeout for TCP connections
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Read timeout for TCP connections
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum retry attempts for write operations
pub const WRITE_MAX_RETRIES: usize = 3;

/// Delay between write retry attempts
pub const WRITE_RETRY_DELAY: Duration = Duration::from_millis(10);

// ============================================================================
// Statistics Constants
// ============================================================================

/// Minimum shares before considering a worker for pruning
pub const MIN_SHARES_FOR_PRUNING: i64 = 0;

/// Time before pruning a worker with no shares (seconds)
pub const WORKER_INITIAL_GRACE_PERIOD: u64 = 180;

/// Time before pruning an inactive worker (seconds)
pub const WORKER_INACTIVITY_TIMEOUT: u64 = 600;

/// Timeout for waiting for writable socket
pub const SOCKET_WAIT_DELAY: Duration = Duration::from_millis(10);
