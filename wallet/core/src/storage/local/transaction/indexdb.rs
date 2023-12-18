//!
//! Web browser IndexedDB implementation of the transaction storage.
//!

use crate::imports::*;
use crate::result::Result;
use crate::storage::interface::{StorageStream, TransactionRangeResult};
use crate::storage::TransactionRecord;
use crate::storage::{Binding, TransactionKind, TransactionRecordStore};

pub struct Inner {
    known_databases: HashMap<String, HashSet<String>>,
}

pub struct TransactionStore {
    inner: Arc<Mutex<Inner>>,
    name: String,
}

impl TransactionStore {
    pub fn new(name: &str) -> TransactionStore {
        TransactionStore { inner: Arc::new(Mutex::new(Inner { known_databases: HashMap::default() })), name: name.to_string() }
    }

    #[inline(always)]
    fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub fn make_db_name(&self, binding: &str, network_id: &str) -> String {
        format!("{}_{}_{}", self.name, binding, network_id)
    }

    pub fn database_is_registered(&self, binding: &str, network_id: &str) -> bool {
        let inner = self.inner();
        if let Some(network_ids) = inner.known_databases.get(binding) {
            network_ids.contains(network_id)
        } else {
            false
        }
    }

    pub fn register_database(&self, binding: &str, network_id: &str) -> Result<()> {
        let mut inner = self.inner();
        if let Some(network_ids) = inner.known_databases.get_mut(binding) {
            network_ids.insert(network_id.to_string());
        } else {
            let mut network_ids = HashSet::new();
            network_ids.insert(network_id.to_string());
            inner.known_databases.insert(binding.to_string(), network_ids);
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn ensure_database(&self, binding: &Binding, network_id: &NetworkId) -> Result<()> {
        let binding_hex = binding.to_hex();
        let network_id = network_id.to_string();
        if !self.database_is_registered(&binding_hex, &network_id) {
            // - TODO
            self.register_database(&binding_hex, &network_id)?;
        }
        Ok(())
    }
}

#[async_trait]
impl TransactionRecordStore for TransactionStore {
    async fn transaction_id_iter(&self, binding: &Binding, network_id: &NetworkId) -> Result<StorageStream<Arc<TransactionId>>> {
        Ok(Box::pin(TransactionIdStream::try_new(self, binding, network_id).await?))
    }

    async fn transaction_data_iter(&self, binding: &Binding, network_id: &NetworkId) -> Result<StorageStream<Arc<TransactionRecord>>> {
        Ok(Box::pin(TransactionRecordStream::try_new(self, binding, network_id).await?))
    }

    async fn load_single(&self, _binding: &Binding, _network_id: &NetworkId, _id: &TransactionId) -> Result<Arc<TransactionRecord>> {
        Err(Error::NotImplemented)
    }

    async fn load_multiple(
        &self,
        _binding: &Binding,
        _network_id: &NetworkId,
        _ids: &[TransactionId],
    ) -> Result<Vec<Arc<TransactionRecord>>> {
        Ok(vec![])
    }

    async fn load_range(
        &self,
        _binding: &Binding,
        _network_id: &NetworkId,
        _filter: Option<Vec<TransactionKind>>,
        _range: std::ops::Range<usize>,
    ) -> Result<TransactionRangeResult> {
        let result = TransactionRangeResult { transactions: vec![], total: 0 };
        Ok(result)
    }

    async fn store(&self, _transaction_records: &[&TransactionRecord]) -> Result<()> {
        Ok(())
    }

    async fn remove(&self, _binding: &Binding, _network_id: &NetworkId, _ids: &[&TransactionId]) -> Result<()> {
        Ok(())
    }

    async fn store_transaction_note(
        &self,
        _binding: &Binding,
        _network_id: &NetworkId,
        _id: TransactionId,
        _note: Option<String>,
    ) -> Result<()> {
        Ok(())
    }
    async fn store_transaction_metadata(
        &self,
        _binding: &Binding,
        _network_id: &NetworkId,
        _id: TransactionId,
        _metadata: Option<String>,
    ) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct TransactionIdStream {}

impl TransactionIdStream {
    pub(crate) async fn try_new(_store: &TransactionStore, _binding: &Binding, _network_id: &NetworkId) -> Result<Self> {
        Ok(Self {})
    }
}

impl Stream for TransactionIdStream {
    type Item = Result<Arc<TransactionId>>;

    #[allow(unused_mut)]
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

#[derive(Clone)]
pub struct TransactionRecordStream {}

impl TransactionRecordStream {
    pub(crate) async fn try_new(_store: &TransactionStore, _binding: &Binding, _network_id: &NetworkId) -> Result<Self> {
        Ok(Self {})
    }
}

impl Stream for TransactionRecordStream {
    type Item = Result<Arc<TransactionRecord>>;

    #[allow(unused_mut)]
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}
