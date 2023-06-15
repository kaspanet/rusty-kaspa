use crate::imports::*;
use crate::iterator::*;
// use crate::storage::local::*;
use crate::result::Result;
use crate::storage::local::interface::LocalStoreInner;
use crate::storage::*;
use async_trait::async_trait;

const DEFAULT_CHUNK_SIZE: usize = 25;

#[derive(Clone)]
struct StoreIteratorInner {
    store: Arc<LocalStoreInner>,
    cursor: usize,
    chunk_size: usize,
    // filter : Option<
}

impl std::fmt::Debug for StoreIteratorInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreIteratorInner").field("cursor", &self.cursor).finish()
    }
}

pub struct KeydataIterator {
    inner: StoreIteratorInner,
}

impl KeydataIterator {
    pub(crate) fn new(store: Arc<LocalStoreInner>, iterator_options: IteratorOptions) -> Self {
        Self { inner: StoreIteratorInner { store, cursor: 0, chunk_size: iterator_options.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE) } }
    }
}

#[async_trait]
impl Iterator for KeydataIterator {
    type Item = Arc<PrvKeyDataInfo>;

    async fn next(&mut self) -> Result<Option<Vec<Self::Item>>> {
        let vec = &self.inner.store.cache().prv_key_data_info.vec;
        if self.inner.cursor >= vec.len() {
            return Ok(None);
        } else {
            let slice = &vec[self.inner.cursor..(self.inner.cursor + self.inner.chunk_size)];
            if slice.is_empty() {
                Ok(None)
            } else {
                self.inner.cursor += slice.len();
                Ok(Some(slice.to_vec()))
            }
        }
    }
}

pub struct AccountIterator {
    inner: StoreIteratorInner,
    filter: Option<PrvKeyDataId>,
}

impl AccountIterator {
    pub(crate) fn new(store: Arc<LocalStoreInner>, filter: Option<PrvKeyDataId>, iterator_options: IteratorOptions) -> Self {
        Self {
            inner: StoreIteratorInner { store, cursor: 0, chunk_size: iterator_options.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE) },
            filter,
        }
    }
}

#[async_trait]
impl Iterator for AccountIterator {
    type Item = Arc<Account>;

    async fn next(&mut self) -> Result<Option<Vec<Self::Item>>> {
        let vec = &self.inner.store.cache().accounts.vec;
        if self.inner.cursor >= vec.len() {
            return Ok(None);
        } else {
            match self.filter {
                None => {
                    let slice = &vec[self.inner.cursor..(self.inner.cursor + self.inner.chunk_size)];
                    if slice.is_empty() {
                        Ok(None)
                    } else {
                        self.inner.cursor += slice.len();
                        Ok(Some(slice.to_vec()))
                    }
                }
                Some(filter) => {
                    let slice = &vec[self.inner.cursor..];

                    let mut accumulator = Vec::new();
                    if slice.is_empty() {
                        return Ok(None);
                    } else {
                        for account in slice {
                            self.inner.cursor += 1;
                            if account.prv_key_data_id == filter {
                                accumulator.push(account.clone());
                                if accumulator.len() >= self.inner.chunk_size {
                                    break;
                                }
                            }
                        }

                        if accumulator.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(accumulator))
                        }
                    }
                }
            }
        }
    }
}

pub struct MetadataIterator {
    inner: StoreIteratorInner,
    filter: Option<PrvKeyDataId>,
}

impl MetadataIterator {
    pub(crate) fn new(store: Arc<LocalStoreInner>, filter: Option<PrvKeyDataId>, iterator_options: IteratorOptions) -> Self {
        Self {
            inner: StoreIteratorInner { store, cursor: 0, chunk_size: iterator_options.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE) },
            filter,
        }
    }
}

#[async_trait]
impl Iterator for MetadataIterator {
    type Item = Arc<Metadata>;

    async fn next(&mut self) -> Result<Option<Vec<Self::Item>>> {
        let vec = &self.inner.store.cache().metadata.vec;
        if self.inner.cursor >= vec.len() {
            return Ok(None);
        } else {
            match self.filter {
                None => {
                    let slice = &vec[self.inner.cursor..(self.inner.cursor + self.inner.chunk_size)];
                    if slice.is_empty() {
                        Ok(None)
                    } else {
                        self.inner.cursor += slice.len();
                        Ok(Some(slice.to_vec()))
                    }
                }
                Some(filter) => {
                    let slice = &vec[self.inner.cursor..];

                    let mut accumulator = Vec::new();
                    if slice.is_empty() {
                        return Ok(None);
                    } else {
                        for account in slice {
                            self.inner.cursor += 1;
                            if account.prv_key_data_id == filter {
                                accumulator.push(account.clone());
                                if accumulator.len() >= self.inner.chunk_size {
                                    break;
                                }
                            }
                        }

                        if accumulator.is_empty() {
                            Ok(None)
                        } else {
                            Ok(Some(accumulator))
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct TransactionRecordIterator {
    inner: StoreIteratorInner,
}

impl TransactionRecordIterator {
    pub(crate) fn new(store: Arc<LocalStoreInner>, iterator_options: IteratorOptions) -> Self {
        Self { inner: StoreIteratorInner { store, cursor: 0, chunk_size: iterator_options.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE) } }
    }
}
#[async_trait]
impl Iterator for TransactionRecordIterator {
    type Item = TransactionRecordId;
    async fn next(&mut self) -> Result<Option<Vec<Self::Item>>> {
        let vec = &self.inner.store.cache().transaction_records.vec;
        if self.inner.cursor >= vec.len() {
            return Ok(None);
        } else {
            let slice = &vec[self.inner.cursor..(self.inner.cursor + self.inner.chunk_size).min(vec.len())];
            if slice.is_empty() {
                Ok(None)
            } else {
                self.inner.cursor += slice.len();
                let vec = slice.iter().map(|data| data.id).collect();
                Ok(Some(vec))
            }
        }
    }
}
