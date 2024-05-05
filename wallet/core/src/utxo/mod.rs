//!
//! UTXO handling primitives.
//!

pub mod balance;
pub mod binding;
pub mod context;
pub mod iterator;
pub mod outgoing;
pub mod pending;
pub mod processor;
pub mod reference;
pub mod scan;
pub mod settings;
pub mod stream;
pub mod sync;

pub use balance::Balance;
pub use binding::UtxoContextBinding;
pub use context::{UtxoContext, UtxoContextId};
pub use iterator::UtxoIterator;
pub use kaspa_consensus_client::UtxoEntryId;
pub use outgoing::OutgoingTransaction;
pub use pending::PendingUtxoEntryReference;
pub use processor::UtxoProcessor;
pub use reference::{Maturity, TryIntoUtxoEntryReferences, UtxoEntryReference, UtxoEntryReferenceExtension};
pub use scan::{Scan, ScanExtent};
pub use settings::*;
pub use stream::UtxoStream;
pub use sync::SyncMonitor;

#[cfg(test)]
pub mod test;
