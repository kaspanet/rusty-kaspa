use crate::imports::*;
use crate::result::Result;
use crate::storage::local::cache::Cache;
use crate::storage::*;

#[derive(Clone)]
struct StoreStreamInner {
    cache: Arc<Mutex<Cache>>,
    cursor: usize,
}

impl StoreStreamInner {
    fn new(cache: Arc<Mutex<Cache>>) -> Self {
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
    pub(crate) fn new(cache: Arc<Mutex<Cache>>) -> Self {
        Self { inner: StoreStreamInner::new(cache) }
    }
}

impl Stream for PrvKeyDataInfoStream {
    type Item = Result<Arc<PrvKeyDataInfo>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cache = self.inner.cache.clone();
        let cache = cache.lock().unwrap();
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
    pub(crate) fn new(cache: Arc<Mutex<Cache>>, filter: Option<PrvKeyDataId>) -> Self {
        Self { inner: StoreStreamInner::new(cache), filter }
    }
}

impl Stream for AccountStream {
    type Item = Result<Arc<Account>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cache = self.inner.cache.clone();
        let cache = cache.lock().unwrap();
        let vec = &cache.accounts.vec;

        if let Some(filter) = self.filter {
            while self.inner.cursor < vec.len() {
                let account = vec[self.inner.cursor].clone();
                self.inner.cursor += 1;
                if account.prv_key_data_id == filter {
                    return Poll::Ready(Some(Ok(account)));
                }
            }
            Poll::Ready(None)
        } else if self.inner.cursor < vec.len() {
            let account = vec[self.inner.cursor].clone();
            self.inner.cursor += 1;
            Poll::Ready(Some(Ok(account)))
        } else {
            Poll::Ready(None)
        }
    }
}

// pub struct MetadataStream {
//     inner: StoreStreamInner,
//     // filter: Option<PrvKeyDataId>,
// }

// impl MetadataStream {
//     pub(crate) fn new(cache: Arc<Mutex<Cache>>, filter: Option<PrvKeyDataId>) -> Self {
//         Self { inner: StoreStreamInner::new(cache), filter }
//     }
// }

// impl Stream for MetadataStream {
//     type Item = Result<Arc<Metadata>>;

//     fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//         let cache = self.inner.cache.clone();
//         let cache = cache.lock().unwrap();
//         let vec = &cache.metadata.vec;

//         if let Some(filter) = self.filter {
//             while self.inner.cursor < vec.len() {
//                 let account = vec[self.inner.cursor].clone();
//                 self.inner.cursor += 1;
//                 if account.prv_key_data_id == filter {
//                     return Poll::Ready(Some(Ok(account)));
//                 }
//             }
//             Poll::Ready(None)
//         } else if self.inner.cursor < vec.len() {
//             let account = vec[self.inner.cursor].clone();
//             self.inner.cursor += 1;
//             Poll::Ready(Some(Ok(account)))
//         } else {
//             Poll::Ready(None)
//         }
//     }
// }

#[derive(Clone, Debug)]
pub struct AddressBookEntryStream {
    inner: StoreStreamInner,
}

impl AddressBookEntryStream {
    pub(crate) fn new(cache: Arc<Mutex<Cache>>) -> Self {
        Self { inner: StoreStreamInner::new(cache) }
    }
}

impl Stream for AddressBookEntryStream {
    type Item = Result<Arc<AddressBookEntry>>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let cache = self.inner.cache.clone();
        let cache = cache.lock().unwrap();
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
