use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{AddressBookEntry, PrvKeyData, PrvKeyDataId};
use kaspa_bip32::Mnemonic;
use zeroize::{Zeroize, ZeroizeOnDrop};

const WALLET_PAYLOAD_MAGIC: u32 = 0x44415441;
const WALLET_PAYLOAD_VERSION: u32 = 0;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub prv_key_data: Vec<PrvKeyData>,
    pub accounts: Vec<AccountStorage>,
    pub address_book: Vec<AddressBookEntry>,
}

impl Payload {
    pub fn new(prv_key_data: Vec<PrvKeyData>, accounts: Vec<AccountStorage>, address_book: Vec<AddressBookEntry>) -> Self {
        Self { prv_key_data, accounts, address_book }
    }
}

impl ZeroizeOnDrop for Payload {}

impl Zeroize for Payload {
    fn zeroize(&mut self) {
        self.prv_key_data.zeroize();
    }
}

impl Payload {
    pub fn add_prv_key_data(
        &mut self,
        mnemonic: Mnemonic,
        payment_secret: Option<&Secret>,
        encryption_kind: EncryptionKind,
    ) -> Result<PrvKeyData> {
        let prv_key_data = PrvKeyData::try_new_from_mnemonic(mnemonic, payment_secret, encryption_kind)?;

        if !self.prv_key_data.iter().any(|existing_key_data| prv_key_data.id == existing_key_data.id) {
            self.prv_key_data.push(prv_key_data.clone());
            Ok(prv_key_data)
        } else {
            Err(Error::custom("private key data id already exists in the wallet"))
        }
    }

    pub fn find_prv_key_data(&self, id: &PrvKeyDataId) -> Option<&PrvKeyData> {
        self.prv_key_data.iter().find(|prv_key_data| prv_key_data.id == *id)
    }
}

impl BorshSerialize for Payload {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(WALLET_PAYLOAD_MAGIC, WALLET_PAYLOAD_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.prv_key_data, writer)?;
        BorshSerialize::serialize(&self.accounts, writer)?;
        BorshSerialize::serialize(&self.address_book, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for Payload {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(WALLET_PAYLOAD_MAGIC)?.try_version(WALLET_PAYLOAD_VERSION)?;
        let prv_key_data = BorshDeserialize::deserialize(buf)?;
        let accounts = BorshDeserialize::deserialize(buf)?;
        let address_book = BorshDeserialize::deserialize(buf)?;

        Ok(Self { prv_key_data, accounts, address_book })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_storage_wallet_payload() -> Result<()> {
        let storable_in = Payload::new(vec![], vec![], vec![]);
        let guard = StorageGuard::new(&storable_in);
        let _storable_out = guard.validate()?;

        Ok(())
    }
}
