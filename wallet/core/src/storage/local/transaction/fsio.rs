//!
//! Local file system transaction storage (native+NodeJS fs IO).
//!

use crate::encryption::*;
use crate::imports::*;
use crate::storage::interface::{StorageStream, TransactionRangeResult};
use crate::storage::TransactionRecord;
use crate::storage::{Binding, TransactionKind, TransactionRecordStore};
use kaspa_utils::hex::ToHex;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};
use workflow_store::fs;

pub struct Inner {
    known_folders: HashSet<String>,
}

pub struct TransactionStore {
    inner: Arc<Mutex<Inner>>,
    folder: PathBuf,
    name: String,
}

impl TransactionStore {
    pub fn new<P: AsRef<Path>>(folder: P, name: &str) -> TransactionStore {
        TransactionStore {
            inner: Arc::new(Mutex::new(Inner { known_folders: HashSet::default() })),
            folder: fs::resolve_path(folder.as_ref().to_str().unwrap()).expect("transaction store folder is invalid"),
            name: name.to_string(),
        }
    }

    #[inline(always)]
    fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    fn make_subfolder(&self, binding: &Binding, network_id: &NetworkId) -> String {
        let name = self.name.as_str();
        let binding_hex = binding.to_hex();
        let network_id = network_id.to_string();
        format!("{name}.transactions/{binding_hex}/{network_id}")
    }

    fn make_folder(&self, binding: &Binding, network_id: &NetworkId) -> PathBuf {
        self.folder.join(self.make_subfolder(binding, network_id))
    }

    async fn ensure_folder(&self, binding: &Binding, network_id: &NetworkId) -> Result<PathBuf> {
        let subfolder = self.make_subfolder(binding, network_id);
        let folder = self.folder.join(&subfolder);
        if !self.inner().known_folders.contains(&subfolder) {
            fs::create_dir_all(&folder).await?;
            self.inner().known_folders.insert(subfolder);
        }
        Ok(folder)
    }

    async fn enumerate(&self, binding: &Binding, network_id: &NetworkId) -> Result<VecDeque<TransactionId>> {
        let folder = self.make_folder(binding, network_id);
        let mut transactions = VecDeque::new();
        match fs::readdir(folder, true).await {
            Ok(mut files) => {
                // we reverse the order of the files so that the newest files are first
                files.sort_by_key(|f| std::cmp::Reverse(f.metadata().unwrap().created()));

                for file in files {
                    if let Ok(id) = TransactionId::from_hex(file.file_name()) {
                        transactions.push_back(id);
                    } else {
                        log_error!("TransactionStore::enumerate(): filename {:?} is not a hash (foreign file?)", file);
                    }
                }

                Ok(transactions)
            }
            Err(e) => {
                if e.code() == Some("ENOENT") {
                    Err(Error::NoRecordsFound)
                } else {
                    log_info!("TransactionStore::enumerate(): error reading folder: {:?}", e);
                    Err(e.into())
                }
            }
        }
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
        let folder = self.make_folder(binding, network_id);
        let path = folder.join(id.to_hex());
        Ok(Arc::new(read(&path, None).await?))
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
            let path = folder.join(id.to_hex());
            match read(&path, None).await {
                Ok(tx) => {
                    transactions.push(Arc::new(tx));
                }
                Err(err) => {
                    log_error!("Error loading transaction {id}: {:?}", err);
                }
            }
        }

        Ok(transactions)
    }

    async fn load_range(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        filter: Option<Vec<TransactionKind>>,
        range: std::ops::Range<usize>,
    ) -> Result<TransactionRangeResult> {
        let folder = self.ensure_folder(binding, network_id).await?;
        let ids = self.enumerate(binding, network_id).await?;
        let mut transactions = vec![];

        let total = if let Some(filter) = filter {
            let mut located = 0;

            for id in ids {
                let path = folder.join(id.to_hex());

                match read(&path, None).await {
                    Ok(tx) => {
                        if filter.contains(&tx.kind()) {
                            if located >= range.start && located < range.end {
                                transactions.push(Arc::new(tx));
                            }

                            located += 1;
                        }
                    }
                    Err(err) => {
                        log_error!("Error loading transaction {id}: {:?}", err);
                    }
                }
            }

            located
        } else {
            let iter = ids.iter().skip(range.start).take(range.len());

            for id in iter {
                let path = folder.join(id.to_hex());
                match read(&path, None).await {
                    Ok(tx) => {
                        transactions.push(Arc::new(tx));
                    }
                    Err(err) => {
                        log_error!("Error loading transaction {id}: {:?}", err);
                    }
                }
            }

            ids.len()
        };

        Ok(TransactionRangeResult { transactions, total: total as u64 })
    }

    async fn store(&self, transaction_records: &[&TransactionRecord]) -> Result<()> {
        for tx in transaction_records {
            let folder = self.ensure_folder(tx.binding(), tx.network_id()).await?;
            let filename = folder.join(tx.id().to_hex());
            write(&filename, tx, None, EncryptionKind::XChaCha20Poly1305).await?;
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

    async fn store_transaction_note(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        id: TransactionId,
        note: Option<String>,
    ) -> Result<()> {
        let folder = self.make_folder(binding, network_id);
        let path = folder.join(id.to_hex());
        let mut transaction = read(&path, None).await?;
        transaction.note = note;
        write(&path, &transaction, None, EncryptionKind::XChaCha20Poly1305).await?;
        Ok(())
    }
    async fn store_transaction_metadata(
        &self,
        binding: &Binding,
        network_id: &NetworkId,
        id: TransactionId,
        metadata: Option<String>,
    ) -> Result<()> {
        let folder = self.make_folder(binding, network_id);
        let path = folder.join(id.to_hex());
        let mut transaction = read(&path, None).await?;
        transaction.metadata = metadata;
        write(&path, &transaction, None, EncryptionKind::XChaCha20Poly1305).await?;
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

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.transactions.len(), Some(self.transactions.len()))
    }
}

#[derive(Clone)]
pub struct TransactionRecordStream {
    transactions: VecDeque<TransactionId>,
    folder: PathBuf,
}

impl TransactionRecordStream {
    pub(crate) async fn try_new(store: &TransactionStore, binding: &Binding, network_id: &NetworkId) -> Result<Self> {
        let folder = store.make_folder(binding, network_id);
        let transactions = store.enumerate(binding, network_id).await?;
        Ok(Self { transactions, folder })
    }
}

impl Stream for TransactionRecordStream {
    type Item = Result<Arc<TransactionRecord>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.transactions.is_empty() {
            Poll::Ready(None)
        } else {
            let id = self.transactions.pop_front().unwrap();
            let path = self.folder.join(id.to_hex());
            match read_sync(&path, None) {
                Ok(transaction_data) => Poll::Ready(Some(Ok(Arc::new(transaction_data)))),
                Err(err) => Poll::Ready(Some(Err(err))),
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.transactions.len(), Some(self.transactions.len()))
    }
}

async fn read(path: &Path, secret: Option<&Secret>) -> Result<TransactionRecord> {
    let bytes = fs::read(path).await?;
    let encryptable = Encryptable::<TransactionRecord>::try_from_slice(bytes.as_slice())?;
    Ok(encryptable.decrypt(secret)?.unwrap())
}

fn read_sync(path: &Path, secret: Option<&Secret>) -> Result<TransactionRecord> {
    let bytes = fs::read_sync(path)?;
    let encryptable = Encryptable::<TransactionRecord>::try_from_slice(bytes.as_slice())?;
    Ok(encryptable.decrypt(secret)?.unwrap())
}

async fn write(path: &Path, record: &TransactionRecord, secret: Option<&Secret>, encryption_kind: EncryptionKind) -> Result<()> {
    let data = if let Some(secret) = secret {
        Encryptable::from(record.clone()).into_encrypted(secret, encryption_kind)?
    } else {
        Encryptable::from(record.clone())
    };
    fs::write(path, &data.try_to_vec()?).await?;
    Ok(())
}
