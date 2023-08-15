pub mod binding;
pub mod context;
pub mod entry;
pub mod processor;
pub mod scan;
pub mod selection;
pub mod stream;

use std::sync::atomic::AtomicU64;

pub use binding::UtxoContextBinding;
pub use context::{UtxoContext, UtxoContextId};
pub use entry::{PendingUtxoEntryReference, TryIntoUtxoEntryReferences, UtxoEntries, UtxoEntry, UtxoEntryId, UtxoEntryReference};
pub use processor::UtxoProcessor;
pub use scan::{Scan, ScanExtent};
pub use selection::UtxoSelectionContext;
pub use stream::UtxoStream;

// static UTXO processing thresholds

/// Maturity period for coinbase transactions.
pub static UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION: AtomicU64 = AtomicU64::new(128);
/// Maturity period for user transactions.
pub static UTXO_MATURITY_PERIOD_USER_TRANSACTION: AtomicU64 = AtomicU64::new(16);
/// Recovery period for UTXOs used in transactions.
pub static UTXO_RECOVERY_PERIOD_SECONDS: AtomicU64 = AtomicU64::new(180);
