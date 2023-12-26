//!
//! Implementation of an [`ActiveAccountMap`] which is a
//! thread-safe map of [`AccountId`] to [`Account`].
//!

use crate::imports::*;

#[derive(Default, Clone)]
pub struct ActiveAccountMap(Arc<Mutex<HashMap<AccountId, Arc<dyn Account>>>>);

impl ActiveAccountMap {
    pub fn inner(&self) -> MutexGuard<HashMap<AccountId, Arc<dyn Account>>> {
        self.0.lock().unwrap()
    }

    pub fn clear(&self) {
        self.inner().clear();
    }

    pub fn len(&self) -> usize {
        self.inner().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner().is_empty()
    }

    pub fn first(&self) -> Option<Arc<dyn Account>> {
        self.inner().values().next().cloned()
    }

    pub fn get(&self, account_id: &AccountId) -> Option<Arc<dyn Account>> {
        self.inner().get(account_id).cloned()
    }

    pub fn contains(&self, account_id: &AccountId) -> bool {
        self.inner().get(account_id).is_some()
    }

    pub fn extend(&self, accounts: Vec<Arc<dyn Account>>) {
        let mut map = self.inner();
        let accounts = accounts.into_iter().map(|a| (*a.id(), a)); //.collect::<Vec<_>>();
        map.extend(accounts);
    }

    pub fn insert(&self, account: Arc<dyn Account>) -> Option<Arc<dyn Account>> {
        self.inner().insert(*account.id(), account)
    }

    pub fn remove(&self, id: &AccountId) {
        self.inner().remove(id);
    }

    pub fn collect(&self) -> Vec<Arc<dyn Account>> {
        self.inner().values().cloned().collect()
    }
}
