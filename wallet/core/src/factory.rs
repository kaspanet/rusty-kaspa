use crate::imports::*;
use crate::result::Result;
use std::sync::OnceLock;

#[async_trait]
pub trait Factory {
    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>>;
}

type FactoryMap = AHashMap<AccountKind, Arc<dyn Factory + Sync + Send + 'static>>;

pub fn factories() -> &'static FactoryMap {
    static FACTORIES: OnceLock<FactoryMap> = OnceLock::new();
    FACTORIES.get_or_init(|| {
        let factories: &[(AccountKind, Arc<dyn Factory + Sync + Send + 'static>)] = &[
            (BIP32_ACCOUNT_KIND.into(), Arc::new(bip32::Ctor {})),
            (LEGACY_ACCOUNT_KIND.into(), Arc::new(legacy::Ctor {})),
            (MULTISIG_ACCOUNT_KIND.into(), Arc::new(multisig::Ctor {})),
        ];

        AHashMap::from_iter(factories.iter().map(|(k, v)| (k.clone(), v.clone())))
    })
}

pub async fn try_load_account(
    wallet: &Arc<Wallet>,
    storage: Arc<AccountStorage>,
    meta: Option<Arc<AccountMetadata>>,
) -> Result<Arc<dyn Account>> {
    let factory = factories().get(&storage.kind).ok_or_else(|| Error::AccountFactoryNotFound(storage.kind.clone()))?;

    factory.try_load(wallet, &storage, meta).await
}
