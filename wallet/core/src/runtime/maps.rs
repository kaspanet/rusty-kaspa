use crate::imports::*;
use crate::runtime::{Account, AccountId};

#[derive(Default, Clone)]
pub struct ActiveAccountMap(Arc<Mutex<HashMap<AccountId, Arc<Account>>>>);

impl ActiveAccountMap {
    pub fn inner(&self) -> MutexGuard<HashMap<AccountId, Arc<Account>>> {
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

    pub fn first(&self) -> Option<Arc<Account>> {
        self.inner().values().next().cloned()
    }

    pub fn get(&self, account_id: &AccountId) -> Option<Arc<Account>> {
        self.inner().get(account_id).cloned()
    }

    pub fn extend(&self, accounts: Vec<Arc<Account>>) {
        let mut map = self.inner();
        let accounts = accounts.into_iter().map(|a| (a.id, a)); //.collect::<Vec<_>>();
        map.extend(accounts);
    }

    pub fn insert(&self, account: Arc<Account>) -> Option<Arc<Account>> {
        self.inner().insert(account.id, account)
    }

    pub fn remove(&self, id: &AccountId) {
        self.inner().remove(id);
    }

    pub fn collect(&self) -> Vec<Arc<Account>> {
        self.inner().values().cloned().collect()
    }
}
