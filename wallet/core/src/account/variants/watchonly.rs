//!
//! Watch-only account implementation
//!

use crate::account::Inner;
use crate::derivation::{AddressDerivationManager, AddressDerivationManagerTrait};
use crate::imports::*;

pub const WATCH_ONLY_ACCOUNT_KIND: &str = "kaspa-watch-only-standard";

pub struct Ctor {}

#[async_trait]
impl Factory for Ctor {
    fn name(&self) -> String {
        "watchonly".to_string()
    }

    fn description(&self) -> String {
        "Kaspa Core watch-only Account".to_string()
    }

    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>> {
        Ok(Arc::new(watchonly::WatchOnly::try_load(wallet, storage, meta).await?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Payload {
    pub xpub_keys: ExtendedPublicKeys,
    pub minimum_signatures: u16,
    pub ecdsa: bool,
}

impl Payload {
    pub fn new(xpub_keys: Arc<Vec<ExtendedPublicKeySecp256k1>>, minimum_signatures: u16, ecdsa: bool) -> Self {
        Self { xpub_keys, minimum_signatures, ecdsa }
    }

    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        Ok(Self::try_from_slice(storage.serialized.as_slice())?)
    }
}

impl Storable for Payload {
    // a unique number used for binary
    // serialization data alignment check
    const STORAGE_MAGIC: u32 = 0x92014137;
    // binary serialization version
    const STORAGE_VERSION: u32 = 0;
}

impl AccountStorable for Payload {}

impl BorshSerialize for Payload {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.xpub_keys, writer)?;
        BorshSerialize::serialize(&self.minimum_signatures, writer)?;
        BorshSerialize::serialize(&self.ecdsa, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for Payload {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let xpub_keys = BorshDeserialize::deserialize(buf)?;
        let minimum_signatures = BorshDeserialize::deserialize(buf)?;
        let ecdsa = BorshDeserialize::deserialize(buf)?;

        Ok(Self { xpub_keys, minimum_signatures, ecdsa })
    }
}

pub struct WatchOnly {
    inner: Arc<Inner>,
    xpub_keys: ExtendedPublicKeys,
    minimum_signatures: u16,
    ecdsa: bool,
    derivation: Arc<AddressDerivationManager>,
}

impl WatchOnly {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        name: Option<String>,
        xpub_keys: ExtendedPublicKeys,
        minimum_signatures: u16,
        ecdsa: bool,
    ) -> Result<Self> {
        let settings = AccountSettings { name, ..Default::default() };

        let public_key = xpub_keys.first().ok_or_else(|| Error::WatchOnlyXpubRequired)?.public_key();

        let storable = Payload::new(xpub_keys.clone(), minimum_signatures, ecdsa);

        let (id, storage_key) = match xpub_keys.len() {
            1 => make_account_hashes(from_watch_only(&public_key)),
            _ => make_account_hashes(from_watch_only_multisig(&None, &storable)),
        };
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        let derivation = match xpub_keys.len() {
            1 => {
                AddressDerivationManager::new(
                    wallet,
                    WATCH_ONLY_ACCOUNT_KIND.into(),
                    &xpub_keys,
                    ecdsa,
                    0,
                    None,
                    1,
                    Default::default(),
                )
                .await?
            }
            _ => {
                AddressDerivationManager::new(
                    wallet,
                    MULTISIG_ACCOUNT_KIND.into(),
                    &xpub_keys,
                    ecdsa,
                    0,
                    Some(u32::MIN),
                    minimum_signatures,
                    Default::default(),
                )
                .await?
            }
        };

        Ok(Self { inner, xpub_keys, minimum_signatures, ecdsa, derivation })
    }

    pub async fn try_load(wallet: &Arc<Wallet>, storage: &AccountStorage, meta: Option<Arc<AccountMetadata>>) -> Result<Self> {
        let storable = Payload::try_load(storage)?;
        let inner = Arc::new(Inner::from_storage(wallet, storage));
        let Payload { xpub_keys, minimum_signatures, ecdsa, .. } = storable;
        let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();

        let derivation = match xpub_keys.len() {
            1 => {
                AddressDerivationManager::new(
                    wallet,
                    WATCH_ONLY_ACCOUNT_KIND.into(),
                    &xpub_keys,
                    ecdsa,
                    0,
                    None,
                    1,
                    address_derivation_indexes,
                )
                .await?
            }
            _ => {
                AddressDerivationManager::new(
                    wallet,
                    MULTISIG_ACCOUNT_KIND.into(),
                    &xpub_keys,
                    ecdsa,
                    0,
                    Some(u32::MIN),
                    minimum_signatures,
                    address_derivation_indexes,
                )
                .await?
            }
        };

        Ok(Self { inner, xpub_keys, minimum_signatures, ecdsa, derivation })
    }

    pub fn get_address_range_for_scan(&self, range: std::ops::Range<u32>) -> Result<Vec<Address>> {
        let receive_addresses = self.derivation.receive_address_manager().get_range_with_args(range.clone(), false)?;
        let change_addresses = self.derivation.change_address_manager().get_range_with_args(range, false)?;
        Ok(receive_addresses.into_iter().chain(change_addresses).collect::<Vec<_>>())
    }

    pub fn xpub_keys(&self) -> &ExtendedPublicKeys {
        &self.xpub_keys
    }
}

#[async_trait]
impl Account for WatchOnly {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        WATCH_ONLY_ACCOUNT_KIND.into()
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Err(Error::WatchOnlyAccount)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn sig_op_count(&self) -> u8 {
        u8::try_from(self.xpub_keys.len()).unwrap()
    }

    fn minimum_signatures(&self) -> u16 {
        self.minimum_signatures
    }

    fn receive_address(&self) -> Result<Address> {
        self.derivation.receive_address_manager().current_address()
    }
    fn change_address(&self) -> Result<Address> {
        self.derivation.change_address_manager().current_address()
    }

    fn to_storage(&self) -> Result<AccountStorage> {
        let settings = self.context().settings.clone();
        let storable = Payload::new(self.xpub_keys.clone(), self.minimum_signatures, self.ecdsa);

        let storage = AccountStorage::try_new(
            WATCH_ONLY_ACCOUNT_KIND.into(),
            self.id(),
            self.storage_key(),
            AssocPrvKeyDataIds::None,
            settings,
            storable,
        )?;

        Ok(storage)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        let metadata = AccountMetadata::new(self.inner.id, self.derivation.address_derivation_meta());
        Ok(Some(metadata))
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor = AccountDescriptor::new(
            WATCH_ONLY_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            AssocPrvKeyDataIds::None,
            self.receive_address().ok(),
            self.change_address().ok(),
        )
        .with_property(AccountDescriptorProperty::XpubKeys, self.xpub_keys.clone().into())
        .with_property(AccountDescriptorProperty::Ecdsa, self.ecdsa.into())
        .with_property(AccountDescriptorProperty::DerivationMeta, self.derivation.address_derivation_meta().into());

        Ok(descriptor)
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Ok(self.clone())
    }

}

impl DerivationCapableAccount for WatchOnly {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait> {
        self.derivation.clone()
    }

    fn account_index(&self) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_storage_watchonly() -> Result<()> {
        let storable_in = Payload::new(vec![make_xpub()].into(), 1, false);
        let guard = StorageGuard::new(&storable_in);
        let storable_out = guard.validate()?;

        assert_eq!(storable_in.minimum_signatures, storable_out.minimum_signatures);
        assert_eq!(storable_in.ecdsa, storable_out.ecdsa);
        assert_eq!(storable_in.xpub_keys.len(), storable_out.xpub_keys.len());
        for idx in 0..storable_in.xpub_keys.len() {
            assert_eq!(storable_in.xpub_keys[idx], storable_out.xpub_keys[idx]);
        }

        Ok(())
    }
}
