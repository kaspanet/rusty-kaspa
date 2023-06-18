use super::{Account, Wallet};
use crate::imports::*;
use crate::iterator::*;
use crate::result::Result;
use crate::storage;
use crate::storage::interface::Interface;
use crate::storage::PrvKeyDataId;
use crate::storage::PrvKeyDataInfo;
use async_trait::async_trait;
use futures::future::join_all;
use kaspa_addresses::Prefix as AddressPrefix;

/// Runtime Account iterator.  This iterator uses a storage iterator to
/// fetch all accounts from the storage, converting them into runtime accounts.
/// If an account already exists in the wallet runtime, the existing instance
/// is returned.
pub struct AccountIterator {
    wallet: Arc<Wallet>,
    store: Arc<dyn Interface>,
    filter: Option<PrvKeyDataId>,
    options: IteratorOptions,
    iter: Option<Box<dyn Iterator<Item = Arc<storage::Account>>>>,
}

impl AccountIterator {
    pub fn new(wallet: &Arc<Wallet>, store: &Arc<dyn Interface>, filter: Option<PrvKeyDataId>) -> AccountIterator {
        AccountIterator { wallet: wallet.clone(), store: store.clone(), filter, options: IteratorOptions::default(), iter: None }
    }

    pub fn new_with_options(
        wallet: &Arc<Wallet>,
        store: &Arc<dyn Interface>,
        filter: Option<PrvKeyDataId>,
        options: IteratorOptions,
    ) -> AccountIterator {
        AccountIterator { wallet: wallet.clone(), store: store.clone(), filter, options, iter: None }
    }

    async fn load_or_create(&self, stored: &storage::Account, prefix: AddressPrefix) -> Result<Arc<Account>> {
        if let Some(account) = self.wallet.active_accounts().get(&stored.id) {
            Ok(account)
        } else {
            Account::try_new_arc_from_storage(&self.wallet, stored, prefix).await
        }
    }
}

#[async_trait]
impl Iterator for AccountIterator {
    type Item = Arc<Account>;

    async fn next(&mut self) -> Result<Option<Vec<Self::Item>>> {
        if self.iter.is_none() {
            self.iter = Some(self.store.clone().as_account_store()?.iter_with_options(self.filter, self.options.clone()).await?);
        }

        // use underlying iterator to fetch accounts
        // if account is already loaded in the wallet, return it
        // otherwise create a new (temporary) instance of the account
        if let Some(accounts) = self.iter.as_mut().unwrap().next().await? {
            let prefix: AddressPrefix = self.wallet.network().into();
            let accounts = accounts.iter().map(|stored| self.load_or_create(stored, prefix)).collect::<Vec<_>>();
            let accounts = join_all(accounts).await.into_iter().collect::<Result<Vec<_>>>()?;
            Ok(Some(accounts))
        } else {
            Ok(None)
        }
    }
}

pub struct PrvKeyDataIterator {
    store: Arc<dyn Interface>,
    options: IteratorOptions,
    iter: Option<Box<dyn Iterator<Item = Arc<storage::PrvKeyDataInfo>>>>,
}

impl PrvKeyDataIterator {
    pub fn new(store: &Arc<dyn Interface>) -> PrvKeyDataIterator {
        Self::new_with_options(store, IteratorOptions::default())
    }
    pub fn new_with_options(store: &Arc<dyn Interface>, options: IteratorOptions) -> PrvKeyDataIterator {
        PrvKeyDataIterator { store: store.clone(), options, iter: None }
    }
}

#[async_trait]
impl Iterator for PrvKeyDataIterator {
    type Item = Arc<PrvKeyDataInfo>;

    async fn next(&mut self) -> Result<Option<Vec<Self::Item>>> {
        if self.iter.is_none() {
            self.iter = Some(self.store.as_prv_key_data_store()?.iter_with_options(self.options.clone()).await?);
        }

        if let Some(keydata) = self.iter.as_mut().unwrap().next().await? {
            Ok(Some(keydata))
        } else {
            Ok(None)
        }
    }
}
