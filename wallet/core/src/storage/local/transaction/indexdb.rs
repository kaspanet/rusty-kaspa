//!
//! Web browser IndexedDB implementation of the transaction storage.
//!

use crate::imports::*;
use crate::result::Result;
use crate::storage::interface::{StorageStream, TransactionRangeResult};
use crate::storage::TransactionRecord;
use crate::storage::{Binding, TransactionKind, TransactionRecordStore};
use indexed_db_futures::prelude::*;
use workflow_core::task::call_async_no_send;

const TRANSACTIONS_STORE_NAME: &str = "transactions";

pub struct Inner {
    known_databases: HashMap<String, HashSet<String>>,
}

impl Inner {
    async fn open_db(&self, db_name: String) -> Result<IdbDatabase> {
        call_async_no_send!(async move {
            let mut db_req: OpenDbRequest = IdbDatabase::open_u32(&db_name, 1)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb database {:?}", err)))?;

            fn on_upgrade_needed(evt: &IdbVersionChangeEvent) -> Result<(), JsValue> {
                // Check if the object store exists; create it if it doesn't
                if let None = evt.db().object_store_names().find(|n| n == TRANSACTIONS_STORE_NAME) {
                    evt.db().create_object_store(TRANSACTIONS_STORE_NAME)?;
                }
                Ok(())
            }

            db_req.set_on_upgrade_needed(Some(on_upgrade_needed));

            db_req.await.map_err(|err| Error::Custom(format!("Open database request failed for indexdb database {:?}", err)))
        })
    }
}

pub struct TransactionStore {
    inner: Arc<Mutex<Arc<Inner>>>,
    name: String,
}

impl TransactionStore {
    pub fn new(name: &str) -> TransactionStore {
        TransactionStore {
            inner: Arc::new(Mutex::new(Arc::new(Inner { known_databases: HashMap::default() }))),
            name: name.to_string(),
        }
    }

    #[inline(always)]
    fn inner(&self) -> MutexGuard<Arc<Inner>> {
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

    pub async fn register_database(&self, binding: &str, network_id: &str) -> Result<()> {
        let db_name = self.make_db_name(binding, network_id);

        let mut inner = self.inner();

        inner.open_db(db_name).await?;

        let mut known_databases = inner.known_databases.clone();

        if let Some(network_ids) = known_databases.get_mut(binding) {
            network_ids.insert(network_id.to_string());
        } else {
            let mut network_ids = HashSet::new();
            network_ids.insert(network_id.to_string());
            known_databases.insert(binding.to_string(), network_ids);
        }

        *inner = Arc::new(Inner { known_databases });

        Ok(())
    }

    #[allow(dead_code)]
    async fn ensure_database(&self, binding: &Binding, network_id: &NetworkId) -> Result<()> {
        let binding_hex = binding.to_hex();
        let network_id = network_id.to_string();
        if !self.database_is_registered(&binding_hex, &network_id) {
            // - TODO
            self.register_database(&binding_hex, &network_id).await?;
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

    async fn load_single(&self, binding: &Binding, network_id: &NetworkId, id: &TransactionId) -> Result<Arc<TransactionRecord>> {
        let binding_str = binding.to_hex();
        let network_id_str = network_id.to_string();
        let id_str = id.to_string();
        let db_name = self.make_db_name(&binding_str, &network_id_str);

        let inner_guard = self.inner.clone();
        let inner = inner_guard.lock().unwrap().clone();

        call_async_no_send!(async move {
            let db = inner.open_db(db_name).await?;

            let idb_tx = db
                .transaction_on_one_with_mode(&TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readonly)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for reading {:?}", err)))?;
            let store = idb_tx
                .object_store(&TRANSACTIONS_STORE_NAME)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for reading {:?}", err)))?;

            let js_value: JsValue = store
                .get_owned(&id_str)
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .await
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .ok_or_else(|| Error::Custom(format!("Transaction record not found in indexdb")))?;

            let transaction_record = TransactionRecord::try_from(js_value)
                .map_err(|err| Error::Custom(format!("Failed to deserialize transaction record from indexdb {:?}", err)))?;

            Ok(Arc::new(transaction_record))
        })
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
