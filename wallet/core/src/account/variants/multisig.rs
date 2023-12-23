//!
//! MultiSig account implementation.
//!

use crate::account::Inner;
use crate::derivation::{AddressDerivationManager, AddressDerivationManagerTrait};
use crate::imports::*;

pub const MULTISIG_ACCOUNT_KIND: &str = "kaspa-multisig-standard";

pub struct Ctor {}

#[async_trait]
impl Factory for Ctor {
    fn name(&self) -> String {
        "multisig".to_string()
    }

    fn description(&self) -> String {
        "Kaspa Core Multi-Signature Account".to_string()
    }

    async fn try_load(
        &self,
        wallet: &Arc<Wallet>,
        storage: &AccountStorage,
        meta: Option<Arc<AccountMetadata>>,
    ) -> Result<Arc<dyn Account>> {
        Ok(Arc::new(MultiSig::try_load(wallet, storage, meta).await?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Payload {
    pub xpub_keys: ExtendedPublicKeys,
    pub cosigner_index: Option<u8>,
    pub minimum_signatures: u16,
    pub ecdsa: bool,
}

impl Payload {
    pub fn new(xpub_keys: ExtendedPublicKeys, cosigner_index: Option<u8>, minimum_signatures: u16, ecdsa: bool) -> Self {
        Self { xpub_keys, cosigner_index, minimum_signatures, ecdsa }
    }

    pub fn try_load(storage: &AccountStorage) -> Result<Self> {
        Ok(Self::try_from_slice(storage.serialized.as_slice())?)
    }
}

impl Storable for Payload {
    const STORAGE_MAGIC: u32 = 0x4749534d;
    const STORAGE_VERSION: u32 = 0;
}

impl AccountStorable for Payload {}

impl BorshSerialize for Payload {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;

        BorshSerialize::serialize(&self.xpub_keys, writer)?;
        BorshSerialize::serialize(&self.cosigner_index, writer)?;
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
        let cosigner_index = BorshDeserialize::deserialize(buf)?;
        let minimum_signatures = BorshDeserialize::deserialize(buf)?;
        let ecdsa = BorshDeserialize::deserialize(buf)?;

        Ok(Self { xpub_keys, cosigner_index, minimum_signatures, ecdsa })
    }
}

pub struct MultiSig {
    inner: Arc<Inner>,
    xpub_keys: ExtendedPublicKeys,
    prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
    cosigner_index: Option<u8>,
    minimum_signatures: u16,
    ecdsa: bool,
    derivation: Arc<AddressDerivationManager>,
}

impl MultiSig {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        name: Option<String>,
        xpub_keys: ExtendedPublicKeys,
        prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
        cosigner_index: Option<u8>,
        minimum_signatures: u16,
        ecdsa: bool,
    ) -> Result<Self> {
        let storable = Payload::new(xpub_keys.clone(), cosigner_index, minimum_signatures, ecdsa);
        let settings = AccountSettings { name, ..Default::default() };
        let (id, storage_key) = make_account_hashes(from_multisig(&prv_key_data_ids, &storable));
        let inner = Arc::new(Inner::new(wallet, id, storage_key, settings));

        let derivation = AddressDerivationManager::new(
            wallet,
            MULTISIG_ACCOUNT_KIND.into(),
            &xpub_keys,
            ecdsa,
            0,
            cosigner_index.map(|v| v as u32),
            minimum_signatures,
            Default::default(),
        )
        .await?;

        Ok(Self { inner, xpub_keys, cosigner_index, minimum_signatures, ecdsa, derivation, prv_key_data_ids })
    }

    pub async fn try_load(wallet: &Arc<Wallet>, storage: &AccountStorage, meta: Option<Arc<AccountMetadata>>) -> Result<Self> {
        let storable = Payload::try_load(storage)?;
        let inner = Arc::new(Inner::from_storage(wallet, storage));

        let Payload { xpub_keys, cosigner_index, minimum_signatures, ecdsa, .. } = storable;

        let address_derivation_indexes = meta.and_then(|meta| meta.address_derivation_indexes()).unwrap_or_default();

        let derivation = AddressDerivationManager::new(
            wallet,
            MULTISIG_ACCOUNT_KIND.into(),
            &xpub_keys,
            ecdsa,
            0,
            cosigner_index.map(|v| v as u32),
            minimum_signatures,
            address_derivation_indexes,
        )
        .await?;

        // TODO @maxim check variants transforms - None->Ok(None), Multiple->Ok(Some()), Single->Err()
        let prv_key_data_ids = storage.prv_key_data_ids.clone().try_into()?;

        Ok(Self { inner, xpub_keys, cosigner_index, minimum_signatures, ecdsa, derivation, prv_key_data_ids })
    }

    pub fn prv_key_data_ids(&self) -> &Option<Arc<Vec<PrvKeyDataId>>> {
        &self.prv_key_data_ids
    }

    pub fn minimum_signatures(&self) -> u16 {
        self.minimum_signatures
    }

    pub fn xpub_keys(&self) -> &ExtendedPublicKeys {
        &self.xpub_keys
    }
}

#[async_trait]
impl Account for MultiSig {
    fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    fn account_kind(&self) -> AccountKind {
        MULTISIG_ACCOUNT_KIND.into()
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Err(Error::AccountKindFeature)
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account> {
        self
    }

    fn sig_op_count(&self) -> u8 {
        // TODO @maxim
        1
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
        let storable = Payload::new(self.xpub_keys.clone(), self.cosigner_index, self.minimum_signatures, self.ecdsa);
        let account_storage = AccountStorage::try_new(
            MULTISIG_ACCOUNT_KIND.into(),
            self.id(),
            self.storage_key(),
            self.prv_key_data_ids.clone().try_into()?,
            settings,
            storable,
        )?;

        Ok(account_storage)
    }

    fn metadata(&self) -> Result<Option<AccountMetadata>> {
        let metadata = AccountMetadata::new(self.inner.id, self.derivation.address_derivation_meta());
        Ok(Some(metadata))
    }

    fn descriptor(&self) -> Result<AccountDescriptor> {
        let descriptor = AccountDescriptor::new(
            MULTISIG_ACCOUNT_KIND.into(),
            *self.id(),
            self.name(),
            self.prv_key_data_ids.clone().try_into()?,
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

impl DerivationCapableAccount for MultiSig {
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
    fn test_storage_multisig() -> Result<()> {
        let storable_in = Payload::new(vec![make_xpub()].into(), Some(42), 0xc0fe, false);
        let guard = StorageGuard::new(&storable_in);
        let storable_out = guard.validate()?;

        assert_eq!(storable_in.cosigner_index, storable_out.cosigner_index);
        assert_eq!(storable_in.minimum_signatures, storable_out.minimum_signatures);
        assert_eq!(storable_in.ecdsa, storable_out.ecdsa);
        assert_eq!(storable_in.xpub_keys.len(), storable_out.xpub_keys.len());
        for idx in 0..storable_in.xpub_keys.len() {
            assert_eq!(storable_in.xpub_keys[idx], storable_out.xpub_keys[idx]);
        }

        Ok(())
    }
}
