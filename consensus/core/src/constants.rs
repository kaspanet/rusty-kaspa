pub const BLOCK_VERSION: u16 = 1;
pub const TX_VERSION: u16 = 0;
pub const LOCK_TIME_THRESHOLD: u64 = 500_000_000_000;
pub const SOMPI_PER_KASPA: u64 = 100_000_000;
pub const MAX_SOMPI: u64 = 29_000_000_000 * SOMPI_PER_KASPA;

// SEQUENCE_LOCK_TIME_MASK is a mask that extracts the relative lock time
// when masked against the transaction input sequence number.
pub const SEQUENCE_LOCK_TIME_MASK: u64 = 0x00000000ffffffff;

// SEQUENCE_LOCK_TIME_DISABLED is a flag that if set on a transaction
// input's sequence number, the sequence number will not be interpreted
// as a relative lock time.
pub const SEQUENCE_LOCK_TIME_DISABLED: u64 = 1 << 63;
