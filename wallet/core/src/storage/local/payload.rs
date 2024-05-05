//!
//! Encrypted wallet payload storage.
//!

use crate::imports::*;
use crate::storage::{AddressBookEntry, PrvKeyData, PrvKeyDataId};
use kaspa_bip32::Mnemonic;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub prv_key_data: Vec<PrvKeyData>,
    pub accounts: Vec<AccountStorage>,
    pub address_book: Vec<AddressBookEntry>,
    pub encrypt_transactions: Option<EncryptionKind>,
}

impl Payload {
    const STORAGE_MAGIC: u32 = 0x41544144;
    const STORAGE_VERSION: u32 = 0;

    pub fn new(prv_key_data: Vec<PrvKeyData>, accounts: Vec<AccountStorage>, address_book: Vec<AddressBookEntry>) -> Self {
        Self { prv_key_data, accounts, address_book, encrypt_transactions: None }
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
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.prv_key_data, writer)?;
        BorshSerialize::serialize(&self.accounts, writer)?;
        BorshSerialize::serialize(&self.address_book, writer)?;
        BorshSerialize::serialize(&self.encrypt_transactions, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for Payload {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;
        let prv_key_data = BorshDeserialize::deserialize(buf)?;
        let accounts = BorshDeserialize::deserialize(buf)?;
        let address_book = BorshDeserialize::deserialize(buf)?;
        let encrypt_transactions = BorshDeserialize::deserialize(buf)?;

        Ok(Self { prv_key_data, accounts, address_book, encrypt_transactions })
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
