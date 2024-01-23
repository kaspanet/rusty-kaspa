//!
//! Web browser IndexedDB implementation of the transaction storage.
//!

use crate::imports::*;
use crate::result::Result;
use crate::storage::interface::{StorageStream, TransactionRangeResult};
use crate::storage::TransactionRecord;
use crate::storage::{Binding, TransactionKind, TransactionRecordStore};
use indexed_db_futures::prelude::*;
use itertools::Itertools;
use js_sys::{Date, Uint8Array};
use workflow_core::task::call_async_no_send;

const TRANSACTIONS_STORE_NAME: &str = "transactions";
const TRANSACTIONS_STORE_ID_INDEX: &str = "id";
const TRANSACTIONS_STORE_TIMESTAMP_INDEX: &str = "timestamp";
const TRANSACTIONS_STORE_DATA_INDEX: &str = "data";

const ENCRYPTION_KIND: EncryptionKind = EncryptionKind::XChaCha20Poly1305;

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
                if !evt.db().object_store_names().any(|n| n == TRANSACTIONS_STORE_NAME) {
                    let object_store = evt.db().create_object_store(TRANSACTIONS_STORE_NAME)?;
                    object_store.create_index_with_params(
                        TRANSACTIONS_STORE_ID_INDEX,
                        &IdbKeyPath::str(TRANSACTIONS_STORE_ID_INDEX),
                        IdbIndexParameters::new().unique(true),
                    )?;
                    object_store.create_index_with_params(
                        TRANSACTIONS_STORE_TIMESTAMP_INDEX,
                        &IdbKeyPath::str(TRANSACTIONS_STORE_TIMESTAMP_INDEX),
                        IdbIndexParameters::new().unique(false),
                    )?;
                    object_store.create_index_with_params(
                        TRANSACTIONS_STORE_DATA_INDEX,
                        &IdbKeyPath::str(TRANSACTIONS_STORE_DATA_INDEX),
                        IdbIndexParameters::new().unique(false),
                    )?;
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
    // name: String,
}

impl TransactionStore {
    pub fn new(_name: &str) -> TransactionStore {
        TransactionStore {
            inner: Arc::new(Mutex::new(Arc::new(Inner { known_databases: HashMap::default() }))),
            // name: name.to_string(),
        }
    }

    #[inline(always)]
    fn inner(&self) -> MutexGuard<Arc<Inner>> {
        self.inner.lock().unwrap()
    }

    pub fn make_db_name(&self, binding: &str, network_id: &str) -> String {
        // format!("{}_{}_{}", self.name, binding, network_id)
        format!("{}_{}", binding, network_id)
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

        let inner = self.inner().clone();

        inner.open_db(db_name).await?;

        let mut inner = self.inner();

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
                .transaction_on_one_with_mode(TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readonly)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for reading {:?}", err)))?;
            let store = idb_tx
                .object_store(TRANSACTIONS_STORE_NAME)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for reading {:?}", err)))?;

            let js_value: JsValue = store
                .get_owned(&id_str)
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .await
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .ok_or_else(|| Error::Custom("Transaction record not found in indexdb".to_string()))?;

            let transaction_record = transaction_record_from_js_value(&js_value, None)
                .map_err(|err| Error::Custom(format!("Failed to deserialize transaction record from indexdb {:?}", err)))?;

            Ok(Arc::new(transaction_record))
        })
    }

    async fn load_multiple(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        ids: &[TransactionId],
    ) -> Result<Vec<Arc<TransactionRecord>>> {
        let binding_str = binding.to_hex();
        let network_id_str = network_id.to_string();
        let db_name = self.make_db_name(&binding_str, &network_id_str);

        let id_strs = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();

        let inner_guard = self.inner.clone();
        let inner = inner_guard.lock().unwrap().clone();

        call_async_no_send!(async move {
            let db = inner.open_db(db_name).await?;

            let idb_tx = db
                .transaction_on_one_with_mode(TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readonly)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for reading {:?}", err)))?;
            let store = idb_tx
                .object_store(TRANSACTIONS_STORE_NAME)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for reading {:?}", err)))?;

            let mut transaction_records = Vec::with_capacity(id_strs.len());
            for id_str in id_strs {
                let js_value: JsValue = store
                    .get_owned(&id_str)
                    .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                    .await
                    .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                    .ok_or_else(|| Error::Custom("Transaction record not found in indexdb".to_string()))?;

                let transaction_record = transaction_record_from_js_value(&js_value, None)
                    .map_err(|err| Error::Custom(format!("Failed to deserialize transaction record from indexdb {:?}", err)))?;
                transaction_records.push(Arc::new(transaction_record));
            }

            Ok(transaction_records)
        })
    }

    async fn load_range(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        _filter: Option<Vec<TransactionKind>>,
        _range: std::ops::Range<usize>,
    ) -> Result<TransactionRangeResult> {
        // log_info!("DEBUG IDB: Loading transaction records for range {:?}", _range);

        let binding_str = binding.to_hex();
        let network_id_str = network_id.to_string();
        let db_name = self.make_db_name(&binding_str, &network_id_str);

        let inner = self.inner().clone();

        call_async_no_send!(async move {
            let db = inner.open_db(db_name).await?;

            let idb_tx = db
                .transaction_on_one_with_mode(TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readonly)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for reading {:?}", err)))?;

            let store = idb_tx
                .object_store(TRANSACTIONS_STORE_NAME)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for reading {:?}", err)))?;

            let array = store
                .get_all()
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .await
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?;

            let transactions = array
                .iter()
                .filter_map(|js_value| match transaction_record_from_js_value(&js_value, None) {
                    Ok(transaction_record) => Some(Arc::new(transaction_record)),
                    Err(err) => {
                        log_error!("Failed to deserialize transaction record from indexdb {:?}", err);
                        None
                    }
                })
                .collect::<Vec<_>>();

            let total = transactions.len() as u64;
            Ok(TransactionRangeResult { transactions, total })
        })
    }

    async fn store(&self, transaction_records: &[&TransactionRecord]) -> Result<()> {
        struct StorableItem {
            db_name: String,
            id: String,
            js_value: JsValue,
        }

        let items = transaction_records
            .iter()
            .map(|transaction_record| {
                let binding_str = transaction_record.binding.to_hex();
                let network_id_str = transaction_record.network_id.to_string();
                let db_name = self.make_db_name(&binding_str, &network_id_str);

                let id = transaction_record.id.to_string();
                let js_value = transaction_record_to_js_value(transaction_record, None, ENCRYPTION_KIND)?;
                Ok(StorableItem { db_name, id, js_value })
            })
            .collect::<Result<Vec<_>>>()?;

        let inner_guard = self.inner.clone();
        let inner = inner_guard.lock().unwrap().clone();

        call_async_no_send!(async move {
            for (db_name, items) in &items.into_iter().group_by(|item| item.db_name.clone()) {
                let db = inner.open_db(db_name).await?;

                let idb_tx = db
                    .transaction_on_one_with_mode(TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readwrite)
                    .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for writing {:?}", err)))?;
                let store = idb_tx
                    .object_store(TRANSACTIONS_STORE_NAME)
                    .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for writing {:?}", err)))?;

                for item in items {
                    store
                        .put_key_val_owned(item.id.as_str(), &item.js_value)
                        .map_err(|_err| Error::Custom("Failed to put transaction record in indexdb object store".to_string()))?;
                }
            }

            Ok(())
        })
    }

    async fn remove(&self, binding: &Binding, network_id: &NetworkId, ids: &[&TransactionId]) -> Result<()> {
        let binding_str = binding.to_hex();
        let network_id_str = network_id.to_string();
        let db_name = self.make_db_name(&binding_str, &network_id_str);

        let id_strs = ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();

        let inner_guard = self.inner.clone();
        let inner = inner_guard.lock().unwrap().clone();

        call_async_no_send!(async move {
            let db = inner.open_db(db_name).await?;

            let idb_tx = db
                .transaction_on_one_with_mode(TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readwrite)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for writing {:?}", err)))?;
            let store = idb_tx
                .object_store(TRANSACTIONS_STORE_NAME)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for writing {:?}", err)))?;

            for id_str in id_strs {
                store
                    .delete_owned(&id_str)
                    .map_err(|_err| Error::Custom("Failed to delete transaction record from indexdb object store".to_string()))?;
            }

            Ok(())
        })
    }

    async fn store_transaction_note(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        id: TransactionId,
        note: Option<String>,
    ) -> Result<()> {
        let binding_str = binding.to_hex();
        let network_id_str = network_id.to_string();
        let id_str = id.to_string();
        let db_name = self.make_db_name(&binding_str, &network_id_str);

        let inner_guard = self.inner.clone();
        let inner = inner_guard.lock().unwrap().clone();

        call_async_no_send!(async move {
            let db = inner.open_db(db_name).await?;

            let idb_tx = db
                .transaction_on_one_with_mode(TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readwrite)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for writing {:?}", err)))?;
            let store = idb_tx
                .object_store(TRANSACTIONS_STORE_NAME)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for writing {:?}", err)))?;

            let js_value: JsValue = store
                .get_owned(&id_str)
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .await
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .ok_or_else(|| Error::Custom("Transaction record not found in indexdb".to_string()))?;

            let mut transaction_record = transaction_record_from_js_value(&js_value, None)
                .map_err(|err| Error::Custom(format!("Failed to deserialize transaction record from indexdb {:?}", err)))?;

            transaction_record.note = note;

            let new_js_value = transaction_record_to_js_value(&transaction_record, None, ENCRYPTION_KIND)?;

            store
                .put_key_val_owned(id_str.as_str(), &new_js_value)
                .map_err(|_err| Error::Custom("Failed to update transaction record in indexdb object store".to_string()))?;

            Ok(())
        })
    }

    async fn store_transaction_metadata(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        id: TransactionId,
        metadata: Option<String>,
    ) -> Result<()> {
        let binding_str = binding.to_hex();
        let network_id_str = network_id.to_string();
        let id_str = id.to_string();
        let db_name = self.make_db_name(&binding_str, &network_id_str);

        let inner_guard = self.inner.clone();
        let inner = inner_guard.lock().unwrap().clone();

        call_async_no_send!(async move {
            let db = inner.open_db(db_name).await?;

            let idb_tx = db
                .transaction_on_one_with_mode(TRANSACTIONS_STORE_NAME, IdbTransactionMode::Readwrite)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb transaction for writing {:?}", err)))?;
            let store = idb_tx
                .object_store(TRANSACTIONS_STORE_NAME)
                .map_err(|err| Error::Custom(format!("Failed to open indexdb object store for writing {:?}", err)))?;

            let js_value: JsValue = store
                .get_owned(&id_str)
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .await
                .map_err(|err| Error::Custom(format!("Failed to get transaction record from indexdb {:?}", err)))?
                .ok_or_else(|| Error::Custom("Transaction record not found in indexdb".to_string()))?;

            let mut transaction_record = transaction_record_from_js_value(&js_value, None)
                .map_err(|err| Error::Custom(format!("Failed to deserialize transaction record from indexdb {:?}", err)))?;

            transaction_record.metadata = metadata;

            let new_js_value = transaction_record_to_js_value(&transaction_record, None, ENCRYPTION_KIND)?;

            store
                .put_key_val_owned(id_str.as_str(), &new_js_value)
                .map_err(|_err| Error::Custom("Failed to update transaction record in indexdb object store".to_string()))?;

            Ok(())
        })
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

fn transaction_record_to_js_value(
    transaction_record: &TransactionRecord,
    secret: Option<&Secret>,
    encryption_kind: EncryptionKind,
) -> Result<JsValue, Error> {
    let id = transaction_record.id.to_string();
    let unixtime_msec = transaction_record.unixtime_msec;
    let mut borsh_data = vec![];
    <TransactionRecord as BorshSerialize>::serialize(transaction_record, &mut borsh_data)?;

    let id_js_value = JsValue::from_str(&id);
    let timestamp_js_value = match unixtime_msec {
        Some(unixtime_msec) => {
            let unixtime_sec = (unixtime_msec / 1000) as u32;

            let date = Date::new_0();
            date.set_utc_seconds(unixtime_sec);
            date.into()
        }
        None => JsValue::NULL,
    };

    let encryped_data = if let Some(secret) = secret {
        Encryptable::from(transaction_record.clone()).into_encrypted(secret, encryption_kind)?
    } else {
        Encryptable::from(transaction_record.clone())
    };
    let encryped_data_vec = encryped_data.try_to_vec()?;
    let borsh_data_uint8_arr = Uint8Array::from(encryped_data_vec.as_slice());
    let borsh_data_js_value = borsh_data_uint8_arr.into();

    let obj = Object::new();
    obj.set("id", &id_js_value)?;
    obj.set("timestamp", &timestamp_js_value)?;
    obj.set("data", &borsh_data_js_value)?;

    let value = JsValue::from(obj);
    Ok(value)
}

fn transaction_record_from_js_value(js_value: &JsValue, secret: Option<&Secret>) -> Result<TransactionRecord, Error> {
    if let Some(object) = Object::try_from(js_value) {
        let borsh_data_jsv = object.get_value("data")?;
        let borsh_data = borsh_data_jsv
            .try_as_vec_u8()
            .map_err(|err| Error::Custom(format!("failed to get blob from transaction record object: {:?}", err)))?;

        let encryptable = Encryptable::<TransactionRecord>::try_from_slice(borsh_data.as_slice())?;
        let transaction_record = encryptable.decrypt(secret)?;

        Ok(transaction_record.0)
    } else {
        Err(Error::Custom("supplied argument must be an object".to_string()))
    }
}
