use derive_more::Deref;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use kaspa_txscript_errors::TxScriptError;

#[allow(dead_code)]
/// Represents an [`Address`] and its matching [`ScriptPublicKey`] representation
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Hash, Deref)]
pub struct UtxoAddress(Address);

impl UtxoAddress {
    pub fn from_address(address: Address) -> Self {
        Self(address)
    }

    pub fn try_from_script(script_public_key: ScriptPublicKey, prefix: Prefix) -> Result<Self, TxScriptError> {
        Ok(Self::from_address(extract_script_pub_key_address(&script_public_key, prefix)?))
    }

    pub fn to_script_public_key(&self) -> ScriptPublicKey {
        pay_to_address_script(self)
    }
}

impl From<Address> for UtxoAddress {
    fn from(address: Address) -> Self {
        Self::from_address(address)
    }
}

impl From<UtxoAddress> for Address {
    fn from(item: UtxoAddress) -> Self {
        item.0
    }
}

pub mod test_helpers {
    use super::*;
    use kaspa_addresses::{Prefix, Version};

    pub fn get_3_addresses(sorted: bool) -> Vec<Address> {
        let mut addresses = vec![
            Address::new(Prefix::Mainnet, Version::PubKey, &[1u8; 32]),
            Address::new(Prefix::Mainnet, Version::PubKey, &[2u8; 32]),
            Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]),
        ];
        if sorted {
            addresses.sort()
        }
        addresses
    }
}
