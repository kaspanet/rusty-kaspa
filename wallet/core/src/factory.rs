//!
//! Wallet Account factories (Account type registration and creation).
//!

use crate::imports::*;
use crate::result::Result;
use std::sync::OnceLock;

#[async_trait]
pub trait Factory {
    fn name(&self) -> String;
    fn description(&self) -> String;
    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>>;
}

type FactoryMap = AHashMap<AccountKind, Arc<dyn Factory + Sync + Send + 'static>>;
static EXTERNAL: OnceLock<Mutex<FactoryMap>> = OnceLock::new();
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn factories() -> &'static FactoryMap {
    static FACTORIES: OnceLock<FactoryMap> = OnceLock::new();
    FACTORIES.get_or_init(|| {
        INITIALIZED.store(true, Ordering::Relaxed);

        let factories: &[(AccountKind, Arc<dyn Factory + Sync + Send + 'static>)] = &[
            (BIP32_ACCOUNT_KIND.into(), Arc::new(bip32::Ctor {})),
            (LEGACY_ACCOUNT_KIND.into(), Arc::new(legacy::Ctor {})),
            (MULTISIG_ACCOUNT_KIND.into(), Arc::new(multisig::Ctor {})),
            (KEYPAIR_ACCOUNT_KIND.into(), Arc::new(keypair::Ctor {})),
        ];

        let external = EXTERNAL.get_or_init(|| Mutex::new(AHashMap::new())).lock().unwrap().clone();

        AHashMap::from_iter(factories.iter().map(|(k, v)| (*k, v.clone())).chain(external))
    })
}

pub fn register(kind: AccountKind, factory: Arc<dyn Factory + Sync + Send + 'static>) {
    if INITIALIZED.load(Ordering::Relaxed) {
        panic!("Factory registrations must occur before the framework initialization");
    }
    let external = EXTERNAL.get_or_init(|| Mutex::new(AHashMap::new()));
    external.lock().unwrap().insert(kind, factory);
}

pub(crate) async fn try_load_account(
    wallet: &Arc<Wallet>,
    storage: Arc<AccountStorage>,
    meta: Option<Arc<AccountMetadata>>,
) -> Result<Arc<dyn Account>> {
    let factory = factories().get(&storage.kind).ok_or_else(|| Error::AccountFactoryNotFound(storage.kind))?;

    factory.try_load(wallet, &storage, meta).await
}
