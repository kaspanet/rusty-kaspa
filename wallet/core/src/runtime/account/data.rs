// use crate::imports::*;
// use crate::result::Result;
// use crate::runtime::Wallet;
// use crate::storage::{self, PrvKeyDataId};
// use crate::AddressDerivationManager;
// use secp256k1::{PublicKey, SecretKey};
// use crate::runtime::account::{AccountId,AccountKind};
// use kaspa_addresses::Version as AddressVersion;

// pub enum AccountData {
//     Legacy {
//         prv_key_data_id: PrvKeyDataId,
//         xpub_keys: Arc<Vec<String>>,

//         derivation: Arc<AddressDerivationManager>,
//     },
//     Bip32 {
//         prv_key_data_id: PrvKeyDataId,
//         account_index : u64,
//         xpub_keys: Arc<Vec<String>>,
//         // cosigner_index: u8,
//         // minimum_signatures: u16,
//         ecdsa: bool,

//         derivation: Arc<AddressDerivationManager>,
//     },
//     MultiSig {
//         prv_key_data_id: PrvKeyDataId,
//         account_index : u64,
//         xpub_keys: Arc<Vec<String>>,
//         cosigner_index: u8,
//         minimum_signatures: u16,
//         ecdsa: bool,

//         derivation: Arc<AddressDerivationManager>,
//     },
//     Secp256k1Keypair {
//         prv_key_data_id: PrvKeyDataId,
//         public_key: PublicKey,
//         ecdsa: bool,
//     },
//     /// Account that is not stored in the database
//     /// and is only used for signing transactions
//     /// during the lifecycle of the runtime.
//     /// Used by Rust and JavaScript runtime APIs.
//     ResidentSecp256k1Keypair {
//         public_key: PublicKey,
//         secret_key: SecretKey,
//     },
// }

// impl AccountData {
//     pub async fn new_from_storage_data(
//         data : &storage::AccountData,
//         wallet: &Arc<Wallet>,
//     ) -> Result<Self> {

//         let data = match data {
//             storage::AccountData::Legacy {
//                 prv_key_data_id,
//                 xpub_keys,
//                 // derivation,
//             } => {
//                 let derivation =
//                     AddressDerivationManager::new(
//                         wallet,
//                         AccountKind::Legacy,
//                         xpub_keys,
//                         false,
//                         None, //0,
//                         None, //1,
//                         None,
//                         None
//                     ).await?;

//                 AccountData::Legacy {
//                     prv_key_data_id: *prv_key_data_id,
//                     xpub_keys: xpub_keys.clone(),
//                     derivation,
//                 }
//             }
//             storage::AccountData::Bip32 {
//                 prv_key_data_id,
//                 account_index,
//                 xpub_keys,
//                 // cosigner_index,
//                 // minimum_signatures,
//                 ecdsa,
//             } => {
//                 let derivation =
//                     AddressDerivationManager::new(
//                         wallet,
//                         AccountKind::Legacy,
//                         xpub_keys,
//                         false,
//                         None, //Some(*cosigner_index,
//                         None, //*minimum_signatures,
//                         None,
//                         None
//                     ).await?;

//                 AccountData::Bip32 {
//                     prv_key_data_id: *prv_key_data_id,
//                     account_index: *account_index,
//                     xpub_keys: xpub_keys.clone(),
//                     // cosigner_index: *cosigner_index,
//                     // minimum_signatures: *minimum_signatures,
//                     ecdsa: *ecdsa,
//                     derivation,
//                 }
//             }
//             storage::AccountData::MultiSig {
//                 prv_key_data_id,
//                 account_index,
//                 xpub_keys,
//                 cosigner_index,
//                 minimum_signatures,
//                 ecdsa,
//             } => {
//                 let derivation =
//                     AddressDerivationManager::new(
//                         wallet,
//                         AccountKind::Legacy,
//                         xpub_keys,
//                         false,
//                         Some(*cosigner_index as u32),
//                         Some(*minimum_signatures as u32),
//                         None,
//                         None
//                     ).await?;
//                 AccountData::MultiSig {
//                     prv_key_data_id: *prv_key_data_id,
//                     account_index: *account_index,
//                     xpub_keys: xpub_keys.clone(),
//                     cosigner_index: *cosigner_index,
//                     minimum_signatures: *minimum_signatures,
//                     ecdsa: *ecdsa,
//                     derivation,
//                 }
//             }
//             storage::AccountData::Secp256k1Keypair {
//                 prv_key_data_id,
//                 public_key,
//                 ecdsa
//             } => AccountData::Secp256k1Keypair {
//                 prv_key_data_id: *prv_key_data_id,
//                 public_key: public_key.clone(),
//                 ecdsa: *ecdsa,
//             },
//         };

//         Ok(data)
//         // let derivation =
//         // AddressDerivationManager::new(wallet, account_kind, &pub_key_data, ecdsa, minimum_signatures, None, None).await?;

//     }

//     pub fn derivation_info(&self) -> Option<DerivationInfo> {

//         match self {
//             AccountData::Legacy {
//                 derivation, ..
//             } => Some(DerivationInfo {
//                 account_kind: AccountKind::Legacy,
//                 account_index: 0,
//                 cosigner_index: 0,
//                 minimum_signatures: 0,
//                 ecdsa: false,
//             }),
//             AccountData::Bip32 {
//                 derivation, account_index, ecdsa, ..
//             } => Some(DerivationInfo {
//                 account_kind: AccountKind::Bip32,
//                 account_index: *account_index,
//                 cosigner_index: 0,
//                 minimum_signatures: 0,
//                 ecdsa: *ecdsa,
//             }),
//             AccountData::MultiSig {
//                 derivation, account_index, cosigner_index, minimum_signatures, ecdsa, ..
//             } => Some(DerivationInfo {
//                 account_kind: AccountKind::MultiSig,
//                 account_index: *account_index,
//                 cosigner_index: *cosigner_index,
//                 minimum_signatures: *minimum_signatures,
//                 ecdsa: *ecdsa,
//             }),
//             AccountData::Secp256k1Keypair {
//                 ..
//             } => None,
//             AccountData::ResidentSecp256k1Keypair {
//                 ..
//             } => None,
//         }

//     }

//     pub fn address_derivation_manager(&self) -> Option<Arc<AddressDerivationManager>> {

//             match self {
//                 AccountData::Legacy {
//                     derivation, ..
//                 } => Some(derivation.clone()),
//                 AccountData::Bip32 {
//                     derivation, ..
//                 } => Some(derivation.clone()),
//                 AccountData::MultiSig {
//                     derivation, ..
//                 } => Some(derivation.clone()),
//                 AccountData::Secp256k1Keypair {
//                     ..
//                 } => None,
//                 AccountData::ResidentSecp256k1Keypair {
//                     ..
//                 } => None,
//             }
//     }

//     pub async fn change_address(&self, network_type: NetworkType) -> Result<Address> {
//         match self {
//             AccountData::Legacy {
//                 derivation, ..
//             } |
//             AccountData::Bip32 {
//                 derivation, ..
//             } |
//             AccountData::MultiSig {
//                 derivation, ..
//             } => derivation.change_address_manager().current_address().await,
//             AccountData::Secp256k1Keypair {
//                 public_key, ..
//             } => {
//                 Ok(Address::new(network_type.into(), AddressVersion::PubKey, &public_key.serialize()[1..]))
//             },
//             AccountData::ResidentSecp256k1Keypair {
//                 public_key, ..
//             } => {
//                 Ok(Address::new(network_type.into(), AddressVersion::PubKey, &public_key.serialize()[1..]))
//             }
//         }
//     }

//     pub async fn receive_address(&self, network_type: NetworkType) -> Result<Address> {
//         match self {
//             AccountData::Legacy {
//                 derivation, ..
//             } |
//             AccountData::Bip32 {
//                 derivation, ..
//             } |
//             AccountData::MultiSig {
//                 derivation, ..
//             } => derivation.receive_address_manager().current_address().await,
//             AccountData::Secp256k1Keypair {
//                 public_key, ..
//             } => {
//                 Ok(Address::new(network_type.into(), AddressVersion::PubKey, &public_key.serialize()[1..]))
//             },
//             AccountData::ResidentSecp256k1Keypair {
//                 public_key, ..
//             } => {
//                 Ok(Address::new(network_type.into(), AddressVersion::PubKey, &public_key.serialize()[1..]))
//             }
//         }
//     }

//     pub fn is_resident(&self) -> bool {

//         match self {
//             AccountData::Legacy { .. } |
//             AccountData::Bip32 { .. } |
//             AccountData::MultiSig { .. } |
//             AccountData::Secp256k1Keypair { .. } => false,
//             AccountData::ResidentSecp256k1Keypair { .. } => true,
//         }

//     }

//     pub fn prv_key_data_id(&self) -> &PrvKeyDataId {

//             match self {
//                 AccountData::Legacy {
//                     prv_key_data_id, ..
//                 } |
//                 AccountData::Bip32 {
//                     prv_key_data_id, ..
//                 } |
//                 AccountData::MultiSig {
//                     prv_key_data_id, ..
//                 } |
//                 AccountData::Secp256k1Keypair {
//                     prv_key_data_id, ..
//                 } => prv_key_data_id,
//                 AccountData::ResidentSecp256k1Keypair { ..
//                 } => panic!("resident account data does not have a stored private key id"),
//             }

//     }

// //     pub fn id(&self) -> AccountId {

// //         match self {
// //             AccountData::Legacy {
// //                 prv_key_data_id, ..
// //             } => AccountId::new(&prv_key_data_id, false, &AccountKind::Legacy, 0),
// //             AccountData::Bip32 {
// //                 prv_key_data_id, account_index, ..
// //             } => AccountId::new(&prv_key_data_id, false, &AccountKind::Bip32, *account_index),
// //             AccountData::MultiSig {
// //                 prv_key_data_id, account_index, ..
// //             } => AccountId::new(&prv_key_data_id, false, &AccountKind::MultiSig, *account_index),
// //             AccountData::Secp256k1Keypair {
// //                 prv_key_data_id, ..
// //             } => AccountId::new(&prv_key_data_id, true, &AccountKind::Secp256k1Keypair, 0),
// //             AccountData::ResidentSecp256k1Keypair {..} => {
// //                 panic!("resident accounts are not allowed in storage")
// // //                AccountId::new(&PrvKeyDataId::from(&PublicKey::from_slice(&[0; 33]).unwrap()), true, &AccountKind::Resident, 0)
// //             },
// //         }

// //     }

// }
