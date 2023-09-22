use crate::runtime::account::AccountId;
use crate::{derivation::AddressDerivationMeta, storage::PrvKeyDataId};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// [`Descriptor`] is a type that offers serializable representation of an account.
/// It is means for RPC or transports that do not allow for transport
/// of the account data.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum Descriptor {
    Bip32 {
        account_id: AccountId,
        prv_key_data_id: PrvKeyDataId,
        account_index: u64,
        xpub_keys: Arc<Vec<String>>,
        ecdsa: bool,
        receive_address: Address,
        change_address: Address,
        meta: AddressDerivationMeta,
    },
    Keypair {
        account_id: AccountId,
        prv_key_data_id: PrvKeyDataId,
        public_key: String,
        ecdsa: bool,
        address: Address,
    },
    Legacy {
        account_id: AccountId,
        prv_key_data_id: PrvKeyDataId,
        xpub_keys: Arc<Vec<String>>,
        receive_address: Address,
        change_address: Address,
        meta: AddressDerivationMeta,
    },
    MultiSig {
        account_id: AccountId,
        // TODO add multisig data
    },
    Resident {
        account_id: AccountId,
        public_key: String,
    },
}
