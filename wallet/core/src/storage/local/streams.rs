//!
//! Async streams for async iteration of wallet primitives.
//!

use crate::imports::*;
use crate::result::Result;
use crate::storage::local::cache::Cache;

#[derive(Clone)]
struct StoreStreamInner {
    cache: Arc<RwLock<Cache>>,
    cursor: usize,
}

impl StoreStreamInner {
    fn new(cache: Arc<RwLock<Cache>>) -> Self {
        Self { cache, cursor: 0 }
    }
}

impl std::fmt::Debug for StoreStreamInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreIteratorInner").field("cursor", &self.cursor).finish()
    }
}

pub struct PrvKeyDataInfoStream {
    inner: StoreStreamInner,
}

impl PrvKeyDataInfoStream {
    pub(crate) fn new(cache: Arc<RwLock<Cache>>) -> Self {
        Self { inner: StoreStreamInner::new(cache) }
    }
}

impl Stream for PrvKeyDataInfoStream {
    type Item = Result<Arc<PrvKeyDataInfo>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cache = self.inner.cache.clone();
        let cache = cache.read().unwrap();
        let vec = &cache.prv_key_data_info.vec;
        if self.inner.cursor < vec.len() {
            let prv_key_data_info = vec[self.inner.cursor].clone();
            self.inner.cursor += 1;
            Poll::Ready(Some(Ok(prv_key_data_info)))
        } else {
            Poll::Ready(None)
        }
    }
}

pub struct AccountStream {
    inner: StoreStreamInner,
    filter: Option<PrvKeyDataId>,
}

impl AccountStream {
    pub(crate) fn new(cache: Arc<RwLock<Cache>>, filter: Option<PrvKeyDataId>) -> Self {
        Self { inner: StoreStreamInner::new(cache), filter }
    }
}

impl Stream for AccountStream {
    type Item = Result<(Arc<AccountStorage>, Option<Arc<AccountMetadata>>)>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cache = self.inner.cache.clone();
        let cache = cache.read().unwrap();
        let accounts = &cache.accounts.vec;
        let metadata = &cache.metadata.map;

        if let Some(filter) = self.filter {
            while self.inner.cursor < accounts.len() {
                let account = accounts[self.inner.cursor].clone();
                self.inner.cursor += 1;

                if account.prv_key_data_ids.contains(&filter) {
                    let meta = metadata.get(&account.id).cloned();
                    return Poll::Ready(Some(Ok((account, meta))));
                } else {
                    continue;
                }
            }
            Poll::Ready(None)
        } else if self.inner.cursor < accounts.len() {
            let account = accounts[self.inner.cursor].clone();
            self.inner.cursor += 1;
            let meta = metadata.get(&account.id).cloned();
            Poll::Ready(Some(Ok((account, meta))))
        } else {
            Poll::Ready(None)
        }
    }
}

#[derive(Clone, Debug)]
pub struct AddressBookEntryStream {
    inner: StoreStreamInner,
}

impl AddressBookEntryStream {
    pub(crate) fn new(cache: Arc<RwLock<Cache>>) -> Self {
        Self { inner: StoreStreamInner::new(cache) }
    }
}

impl Stream for AddressBookEntryStream {
    type Item = Result<Arc<AddressBookEntry>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cache = self.inner.cache.clone();
        let cache = cache.read().unwrap();
        let vec = &cache.address_book; //transaction_records.vec;

        if self.inner.cursor < vec.len() {
            let address_book_entry = vec[self.inner.cursor].clone();
            self.inner.cursor += 1;
            Poll::Ready(Some(Ok(Arc::new(address_book_entry))))
        } else {
            Poll::Ready(None)
        }
    }
}
