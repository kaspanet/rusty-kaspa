use crate::imports::*;
use crate::result::Result;
use crate::storage::interface::StorageStream;
use crate::storage::{Binding, TransactionRecordStore};
use crate::storage::{TransactionMetadata, TransactionRecord};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};
use workflow_store::fs;

pub struct Inner {
    known_folders: HashMap<String, HashSet<String>>,
}

pub struct TransactionStore {
    inner: Arc<Mutex<Inner>>,
    folder: PathBuf,
    name: String,
}

impl TransactionStore {
    pub fn new<P: AsRef<Path>>(folder: P, name: &str) -> TransactionStore {
        TransactionStore {
            inner: Arc::new(Mutex::new(Inner { known_folders: HashMap::default() })),
            folder: fs::resolve_path(folder.as_ref().to_str().unwrap()).expect("transaction store folder is invalid"),
            name: name.to_string(),
        }
    }

    #[inline(always)]
    fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    fn folder_is_registered(&self, binding: &str, network_id: &str) -> bool {
        let inner = self.inner();
        if let Some(network_ids) = inner.known_folders.get(binding) {
            network_ids.contains(network_id)
        } else {
            false
        }
    }

    fn register_folder(&self, binding: &str, network_id: &str) -> Result<()> {
        let mut inner = self.inner();
        if let Some(network_ids) = inner.known_folders.get_mut(binding) {
            network_ids.insert(network_id.to_string());
        } else {
            let mut network_ids = HashSet::new();
            network_ids.insert(network_id.to_string());
            inner.known_folders.insert(binding.to_string(), network_ids);
        }

        Ok(())
    }

    async fn ensure_folder(&self, binding: &Binding, network_id: &NetworkId) -> Result<PathBuf> {
        let binding_hex = binding.to_hex();
        let network_id = network_id.to_string();
        let folder = self.folder.join(&format!("{}.transactions", self.name)).join(&binding_hex).join(&network_id);
        if !self.folder_is_registered(&binding_hex, &network_id) {
            fs::create_dir_all(&folder).await?;
            self.register_folder(&binding_hex, &network_id)?;
        }
        Ok(folder)
    }

    async fn enumerate(&self, binding: &Binding, network_id: &NetworkId) -> Result<VecDeque<TransactionId>> {
        let folder = self.ensure_folder(binding, network_id).await?;
        let folder = folder.to_str().unwrap().to_string();
        let mut transactions = VecDeque::new();
        match fs::readdir(folder).await {
            Ok(files) => {
                for file in files {
                    if let Ok(id) = TransactionId::from_hex(file.file_name()) {
                        transactions.push_back(id);
                    } else {
                        log_error!("TransactionStore::enumerate(): filename {:?} is not a hash (foreign file?)", file);
                    }
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        Ok(transactions)
    }

    pub async fn store_transaction_metadata(&self, _id: TransactionId, _metadata: TransactionMetadata) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl TransactionRecordStore for TransactionStore {
    async fn transaction_id_iter(&self, binding: &Binding, network_id: &NetworkId) -> Result<StorageStream<TransactionId>> {
        Ok(Box::pin(TransactionIdStream::try_new(self, binding, network_id).await?))
    }

    // async fn transaction_iter(&self, binding: &Binding, network_id: &NetworkId) -> Result<StorageStream<TransactionRecord>> {
    //     Ok(Box::pin(TransactionRecordStream::try_new(&self.transactions, binding, network_id).await?))
    // }

    async fn load_single(&self, binding: &Binding, network_id: &NetworkId, id: &TransactionId) -> Result<Arc<TransactionRecord>> {
        let folder = self.ensure_folder(binding, network_id).await?;
        let path = folder.join(id.to_hex());
        Ok(Arc::new(fs::read_json::<TransactionRecord>(&path).await?))
    }

    async fn load_multiple(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        ids: &[TransactionId],
    ) -> Result<Vec<Arc<TransactionRecord>>> {
        let folder = self.ensure_folder(binding, network_id).await?;
        let mut transactions = vec![];

        for id in ids {
            let path = folder.join(&id.to_hex());
            let tx: TransactionRecord = fs::read_json(&path).await?;
            transactions.push(Arc::new(tx));
        }

        Ok(transactions)
    }

    async fn store(&self, transaction_records: &[&TransactionRecord]) -> Result<()> {
        for tx in transaction_records {
            let folder = self.ensure_folder(tx.binding(), tx.network_id()).await?;
            let filename = folder.join(tx.id().to_hex());
            fs::write_json(&filename, tx).await?;
        }

        Ok(())
    }

    async fn remove(&self, binding: &Binding, network_id: &NetworkId, ids: &[&TransactionId]) -> Result<()> {
        let folder = self.ensure_folder(binding, network_id).await?;
        for id in ids {
            let filename = folder.join(id.to_hex());
            fs::remove(&filename).await?;
        }

        Ok(())
    }

    async fn store_transaction_metadata(&self, _id: TransactionId, _metadata: TransactionMetadata) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct TransactionIdStream {
    transactions: VecDeque<TransactionId>,
}

impl TransactionIdStream {
    pub(crate) async fn try_new(store: &TransactionStore, binding: &Binding, network_id: &NetworkId) -> Result<Self> {
        let transactions = store.enumerate(binding, network_id).await?;
        Ok(Self { transactions })
    }
}

impl Stream for TransactionIdStream {
    type Item = Result<Arc<TransactionId>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.transactions.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Ready(Some(Ok(self.transactions.pop_front().map(Arc::new).unwrap())))
        }
    }
}

/*
#[derive(Clone)]
pub struct TransactionRecordStream {
    store: Arc<TransactionStore>,
    folder: PathBuf,
    transactions: VecDeque<TransactionId>,
}

impl TransactionRecordStream {
    pub(crate) async fn try_new(store: &Arc<TransactionStore>, binding: &Binding, network_id: &NetworkId) -> Result<Self> {
        let folder = store.make_folder(binding, network_id)?;
        let transactions = store.enumerate(binding, network_id).await?;
        Ok(Self { store: store.clone(), folder, transactions })
    }
}

impl Stream for TransactionRecordStream {
    type Item = Result<Arc<TransactionRecord>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.transactions.is_empty() {
            Poll::Ready(None)
        } else {
            let id = self.transactions.pop_front().unwrap();
            let filename = id.to_hex();
            let path = self.folder.join(filename);
            match fs::read_json::<TransactionRecord>(&path).await {
                Ok(tx) => Poll::Ready(Some(Ok(Arc::new(tx)))),
                Err(e) => Poll::Ready(Some(Err(e))),
            }
        }
    }
}
*/
