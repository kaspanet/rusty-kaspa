#![allow(dead_code)]

use crate::imports::*;
use crate::result::Result;
use crate::runtime::account::Inner;
use crate::runtime::account::{AccountId, AccountKind};
use crate::runtime::Wallet;
use crate::storage::{self, PrvKeyDataId};
use crate::AddressDerivationManager;
// use kaspa_addresses::Version as AddressVersion;
// use secp256k1::{PublicKey, SecretKey};

pub struct MultiSig {
    inner: Arc<Inner>,

    prv_key_data_id: PrvKeyDataId,
    account_index: u64,
    xpub_keys: Arc<Vec<String>>,
    cosigner_index: u8,
    minimum_signatures: u16,
    ecdsa: bool,
    derivation: Arc<AddressDerivationManager>,
}

impl MultiSig {
    pub async fn try_new(
        wallet: &Arc<Wallet>,
        prv_key_data_id: &PrvKeyDataId,
        settings: &storage::account::Settings,
        data: &storage::account::MultiSig,
    ) -> Result<Self> {
        let id = AccountId::from_multisig(prv_key_data_id, data);
        let inner = Arc::new(Inner::new(wallet, id, Some(settings)));

        let storage::account::MultiSig {
            // prv_key_data_id,
            account_index,
            xpub_keys,
            cosigner_index,
            minimum_signatures,
            ecdsa,
        } = data;

        let derivation = AddressDerivationManager::new(
            wallet,
            AccountKind::Legacy,
            xpub_keys,
            false,
            Some(*cosigner_index as u32),
            Some(*minimum_signatures as u32),
            None,
            None,
        )
        .await?;

        Ok(Self {
            inner,
            prv_key_data_id: prv_key_data_id.clone(),
            account_index: *account_index,
            xpub_keys: xpub_keys.clone(),
            cosigner_index: *cosigner_index,
            minimum_signatures: *minimum_signatures,
            ecdsa: *ecdsa,
            derivation,
            // prv_key_data_id: data.prv_key_data_id.clone(),
            // account_index : data.account_index,
            // xpub_keys: data.xpub_keys.clone(),
            // ecdsa: data.ecdsa,
            // derivation,
        })
    }
}
