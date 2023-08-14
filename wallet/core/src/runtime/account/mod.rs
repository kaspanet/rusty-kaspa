pub mod data;
pub mod id;
pub mod kind;
pub mod variants;

pub use data::*;
pub use id::*;
use kaspa_bip32::ChildNumber;
pub use kind::*;
use secp256k1::ONE_KEY;
pub use variants::*;

#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::derivation::build_derivate_paths;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{Balance, BalanceStrings, Wallet};
use crate::secret::Secret;
use crate::storage::interface::AccessContext;
use crate::storage::{self, AccessContextT, PrvKeyData, PrvKeyDataId};
use crate::tx::{Fees, Generator, GeneratorSettings, GeneratorSummary, KeydataSigner, PaymentDestination, PendingTransaction, Signer};
use crate::utxo::{UtxoContext, UtxoContextBinding, UtxoEntryReference};
// use crate::AddressDerivationManager;
// use faster_hex::hex_string;
// use futures::future::join_all;
// use kaspa_bip32::ChildNumber;
use kaspa_notify::listener::ListenerId;
// use secp256k1::{ONE_KEY, PublicKey, SecretKey};
use separator::Separatable;
// use serde::Serializer;
// use std::hash::Hash;
// use std::str::FromStr;
use workflow_core::abortable::Abortable;
// use workflow_core::enums::u8_try_from;
// use kaspa_addresses::Version as AddressVersion;
use crate::derivation::AddressDerivationManager;

use super::AtomicBalance;

pub const DEFAULT_AMOUNT_PADDING: usize = 19;

pub type GenerationNotifier = Arc<dyn Fn(&PendingTransaction) + Send + Sync>;
pub type DeepScanNotifier = Arc<dyn Fn(usize, u64, Option<TransactionId>) + Send + Sync>;

pub struct Context {
    pub settings: Option<storage::account::Settings>,
    // pub name : Option<String>,
    // pub title: Option<String>,
    pub listener_id: Option<ListenerId>,
    //    pub stored: Option<storage::Account>,
}

pub struct Inner {
    context: Mutex<Context>,
    id: AccountId,
    wallet: Arc<Wallet>,
    utxo_context: UtxoContext,
    // is_connected: AtomicBool,
    // data : AccountData,
}

impl Inner {
    pub fn new(wallet: &Arc<Wallet>, id: AccountId, settings: Option<&storage::account::Settings>) -> Self {
        let utxo_context = UtxoContext::new(wallet.utxo_processor(), UtxoContextBinding::AccountId(id));

        let context = Context { listener_id: None, settings: settings.cloned() };
        Inner { context: Mutex::new(context), id, wallet: wallet.clone(), utxo_context: utxo_context.clone() }
    }
}

pub async fn try_from_storage(wallet: &Arc<Wallet>, stored_account: &Arc<storage::Account>) -> Result<Arc<dyn Account>> {
    // let id = stored_account.id.clone();
    // let settings = stored_account.settings.clone();
    match &stored_account.data {
        storage::AccountData::Bip32(bip32) => {
            Ok(Arc::new(Bip32::try_new(wallet, &stored_account.prv_key_data_id, &stored_account.settings, bip32).await?))
        }
        _ => unimplemented!(),
    }
}

#[async_trait]
pub trait Account: AnySync + Send + Sync + 'static {
    fn inner(&self) -> &Arc<Inner>;

    fn context(&self) -> MutexGuard<Context> {
        self.inner().context.lock().unwrap()
    }

    fn id_ref(&self) -> &AccountId {
        &self.inner().id
    }

    fn account_kind(&self) -> AccountKind;

    fn wallet(&self) -> &Arc<Wallet> {
        &self.inner().wallet
    }

    fn utxo_context(&self) -> &UtxoContext {
        &self.inner().utxo_context
    }

    fn balance(&self) -> Option<Balance> {
        self.utxo_context().balance()
    }

    fn balance_as_strings(&self, padding: Option<usize>) -> Result<BalanceStrings> {
        Ok(BalanceStrings::from((&self.balance(), &self.wallet().network_id()?.into(), padding)))
    }

    fn name(&self) -> Option<String> {
        self.context().settings.as_ref().and_then(|settings| settings.name.clone())
    }

    fn title(&self) -> Option<String> {
        self.context().settings.as_ref().and_then(|settings| settings.title.clone())
    }

    fn name_or_id(&self) -> String {
        if let Some(name) = self.name() {
            // compensate for an empty name
            if name.is_empty() {
                self.id_ref().short()
            } else {
                name
            }
        } else {
            self.id_ref().short()
        }
    }

    fn name_with_id(&self) -> String {
        if let Some(name) = self.name() {
            // compensate for an empty name
            if name.is_empty() {
                self.id_ref().short()
            } else {
                format!("{name} {}", self.id_ref().short())
            }
        } else {
            self.id_ref().short()
        }
    }

    async fn rename(&self, secret: Secret, name: Option<&str>) -> Result<()> {
        // todo!();

        // let stored =
        {
            let mut context = self.context();
            if let Some(settings) = &mut context.settings {
                settings.name = name.map(String::from);
            } else {
                context.settings = Some(storage::Settings { name: name.map(String::from), title: None, ..Default::default() });
            }

            // let settings = self.context().settings.as_ref().ok_or(Error::ResidentAccount)?;
            //     let mut stored = context.stored.clone().ok_or(Error::ResidentAccount)?;
            // context.settings.name = name.map(String::from);
            //     stored
        }

        let stored_account = self.as_storable()?;
        self.wallet().store().as_account_store()?.store(&[&stored_account]).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(secret));
        self.wallet().store().commit(&ctx).await?;
        Ok(())
    }

    fn get_list_string(&self) -> Result<String> {
        let name = style(self.name_with_id()).blue();
        let balance = self.balance_as_strings(None)?;
        let mature_utxo_size = self.utxo_context().mature_utxo_size();
        let pending_utxo_size = self.utxo_context().pending_utxo_size();
        let info = match (mature_utxo_size, pending_utxo_size) {
            (0, 0) => "".to_string(),
            (_, 0) => {
                format!("{} UTXOs", mature_utxo_size.separated_string())
            }
            (0, _) => {
                format!("{} UTXOs pending", pending_utxo_size.separated_string())
            }
            _ => {
                format!("{} UTXOs, {} UTXOs pending", mature_utxo_size.separated_string(), pending_utxo_size.separated_string())
            }
        };
        Ok(format!("{name}: {balance}   {}", style(info).dim()))
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        Err(Error::ResidentAccount)
        // panic!("account type does not have a private key in storage")
    }

    async fn prv_key_data(&self, wallet_secret: Secret) -> Result<PrvKeyData> {
        let prv_key_data_id = self.prv_key_data_id()?; //.ok_or(Error::ResidentAccount)?;

        let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        let keydata = self
            .wallet()
            .store()
            .as_prv_key_data_store()?
            .load_key_data(&access_ctx, prv_key_data_id)
            .await?
            .ok_or(Error::PrivateKeyNotFound(prv_key_data_id.to_hex()))?;
        Ok(keydata)
    }

    fn as_storable(&self) -> Result<storage::Account>;

    async fn scan(self: Arc<Self>, window_size: Option<usize>, extent: Option<u32>) -> Result<()> {
        self.utxo_context().clear().await?;

        match self.clone().as_derivation_capable() {
            Ok(account) => {
                let derivation = account.derivation();

                let current_daa_score = self.wallet().current_daa_score().ok_or(Error::NotConnected)?;
                let balance = Arc::new(AtomicBalance::default());

                let extent = match extent {
                    Some(depth) => ScanExtent::Depth(depth),
                    None => ScanExtent::EmptyWindow,
                };

                let scans = vec![
                    Scan::new_with_args(derivation.receive_address_manager(), window_size, extent, &balance, current_daa_score),
                    Scan::new_with_args(derivation.change_address_manager(), window_size, extent, &balance, current_daa_score),
                ];

                let futures = scans.iter().map(|scan| scan.scan(self.utxo_context())).collect::<Vec<_>>();

                join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
            }
            Err(_) => {}
        }

        // match self.data() {
        //     AccountData::Legacy { derivation, .. }
        //     | AccountData::Bip32 { derivation, .. }
        //     | AccountData::MultiSig { derivation, .. } => {

        //     }
        //     AccountData::ResidentSecp256k1Keypair { public_key, .. } => {
        //         let payload = &public_key.serialize()[1..];
        //         let address = Address::new(self.wallet().network_id()?.network_type.into(), AddressVersion::PubKey, payload);

        //     },

        //     AccountData::Secp256k1Keypair { public_key, .. } => {

        //     }
        // }

        self.utxo_context().update_balance().await?;

        Ok(())
    }

    fn sig_op_count(&self) -> u8 {
        1
    }

    fn minimum_signatures(&self) -> u16 {
        1
    }

    // fn cosigner_index(&self) -> u8 {
    //     0
    // }

    async fn receive_address(&self) -> Result<Address>;

    // async fn receive_address(self : Arc<Self>) -> Result<Address> {
    //     let account = self.as_derivation_capable()?;
    //     Ok(DerivationCapableAccount::receive_address(account).await?)
    // }
    //  {
    //     self.receive_address_manager()?.current_address().await
    // }

    async fn change_address(&self) -> Result<Address>;
    // async fn change_address(self : Arc<Self>) -> Result<Address> {
    //     let account = self.as_derivation_capable()?;
    //     Ok(DerivationCapableAccount::change_address(account).await?)
    // }
    //  {
    //     self.change_address_manager()?.current_address().await
    // }

    // pub fn receive_address_manager(&self) -> Result<Arc<AddressManager>> {
    //     Ok(self.derivation.receive_address_manager())
    // }

    // pub fn change_address_manager(&self) -> Result<Arc<AddressManager>> {
    //     Ok(self.derivation.change_address_manager())
    // }

    // async fn new_receive_address(self: Arc<Self>) -> Result<Address> {
    //     let account = self.as_derivation_capable()?;
    //     Ok(DerivationCapableAccount::new_receive_address(account).await?)
    // }
    //  {
    //     let address = self.receive_address_manager()?.new_address().await?;
    //     self.utxo_context().register_addresses(&[address.clone()]).await?;
    //     Ok(address.into())
    // }

    // async fn new_change_address(self: Arc<Self>) -> Result<Address> {
    //     let account = self.as_derivation_capable()?;
    //     Ok(DerivationCapableAccount::new_change_address(account).await?)
    // }

    //  {
    //     let address = self.change_address_manager()?.new_address().await?;
    //     self.utxo_context().register_addresses(&[address.clone()]).await?;
    //     Ok(address.into())
    // }

    /// Start Account service task
    async fn start(self: Arc<Self>) -> Result<()> {
        // if self.wallet.is_connected() {
        self.connect().await?;
        // }

        Ok(())
    }

    /// Stop Account service task
    async fn stop(self: Arc<Self>) -> Result<()> {
        self.utxo_context().clear().await?;
        self.disconnect().await?;
        Ok(())
    }

    /// handle connection event
    async fn connect(self: Arc<Self>) -> Result<()> {
        let vacated = self.wallet().active_accounts().insert(self.clone().as_dyn_arc());
        if vacated.is_none() && self.wallet().is_connected() {
            self.scan(None, None).await?;
        }
        Ok(())
    }

    /// handle disconnection event
    async fn disconnect(&self) -> Result<()> {
        self.wallet().active_accounts().remove(self.id_ref());
        Ok(())
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account>; // where Self : Sized;

    // fn test(self: &Arc<Self>) -> Arc<dyn Account> where Self : Sized;

    async fn sweep(
        self: Arc<Self>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        // let this : Arc<dyn Account> = self.clone();
        // let this = self.clone().as_dyn_arc();
        // let this : Arc<dyn Account> = self.downcast_ref(); //self.clone();

        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));
        let settings =
            GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), PaymentDestination::Change, Fees::None, None).await?;
        let generator = Generator::new(settings, Some(signer), abortable);

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }

            transaction.try_sign()?;
            transaction.log().await?;
            let id = transaction.try_submit(self.wallet().rpc()).await?;
            ids.push(id);
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    async fn send(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        // let this = self.clone().as_dyn_arc();

        let keydata = self.prv_key_data(wallet_secret).await?;

        // let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        // let keydata = self
        //     .wallet
        //     .store()
        //     .as_prv_key_data_store()?
        //     .load_key_data(&access_ctx, &self.prv_key_data_id)
        //     .await?
        //     .ok_or(Error::PrivateKeyNotFound(self.prv_key_data_id.to_hex()))?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));

        let settings =
            GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), destination, priority_fee_sompi, payload).await?;

        let generator = Generator::new(settings, Some(signer), abortable);

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }

            transaction.try_sign()?;
            transaction.log().await?;
            let id = transaction.try_submit(self.wallet().rpc()).await?;
            ids.push(id);
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    async fn estimate(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        abortable: &Abortable,
    ) -> Result<GeneratorSummary> {
        let settings = GeneratorSettings::try_new_with_account(self.as_dyn_arc(), destination, priority_fee_sompi, payload).await?;

        let generator = Generator::new(settings, None, abortable);

        let mut stream = generator.stream();
        while let Some(_transaction) = stream.try_next().await? {
            _transaction.log().await?;
            yield_executor().await;
        }

        Ok(generator.summary())
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountAddressDerivationCaps)
    }
}

downcast_sync!(dyn Account);

// pub struct DerivationInfo {
//     pub account_kind: AccountKind,
//     pub account_index: u64,
//     pub cosigner_index: u8,
//     pub minimum_signatures: u16,
//     pub ecdsa: bool,
// }

#[async_trait]
pub trait DerivationCapableAccount: Account {
    fn derivation(&self) -> &Arc<AddressDerivationManager>;

    fn account_index(&self) -> u64 {
        0
    }

    // fn derivation_info(&self) -> DerivationInfo;

    async fn derivation_scan(
        self: Arc<Self>,
        wallet_secret: Secret,
        _payment_secret: Option<Secret>,
        extent: usize,
        window: usize,
        sweep: bool,
        abortable: &Abortable,
        notifier: Option<DeepScanNotifier>,
    ) -> Result<()> {
        let derivation = self.derivation();
        // let cosigner_inedex = self.cosigner_index();
        // let account_index = self.account_index();

        let _prv_key_data = self.prv_key_data(wallet_secret).await?;
        let change_address = derivation.change_address_manager().current_address().await?;

        let mut index: usize = 0;
        let mut last_notification = 0;
        let mut aggregate_balance = 0;

        while index < extent {
            let first = index as u32;
            let last = (index + window) as u32;
            index = last as usize;

            // ----
            // - _keydata is initialized above ^
            // - TODO - generate pairs of private keys and addresses as a (Address, secp256k1::Secret) tuple without updating address indexes
            let mut keypairs = derivation.receive_address_manager().get_range(first..last).await?;
            let change_keypairs = derivation.change_address_manager().get_range(first..last).await?;
            keypairs.extend(change_keypairs);
            let keypairs: Vec<(Address, secp256k1::SecretKey)> =
                keypairs.into_iter().map(|address| (address.clone(), ONE_KEY)).collect();

            // ----

            let addresses = keypairs.iter().map(|(address, _)| address.clone()).collect::<Vec<_>>();
            let utxos = self.wallet().rpc().get_utxos_by_addresses(addresses).await?;
            let balance = utxos.iter().map(|utxo| utxo.utxo_entry.amount).sum::<u64>();
            if balance > 0 {
                aggregate_balance += balance;

                if sweep {
                    // TODO - populate with keypairs ^^^
                    let keydata: Vec<(Address, secp256k1::SecretKey)> = vec![];
                    let signer = Arc::new(KeydataSigner::new(keydata));

                    let utxos = utxos.into_iter().map(UtxoEntryReference::from).collect::<Vec<_>>();
                    let settings = GeneratorSettings::try_new_with_iterator(
                        Box::new(utxos.into_iter()),
                        None,
                        1,
                        1,
                        change_address.clone(),
                        PaymentDestination::Change,
                        Fees::None,
                        None,
                    )?;

                    let generator = Generator::new(settings, Some(signer), abortable);

                    let mut stream = generator.stream();
                    while let Some(transaction) = stream.try_next().await? {
                        transaction.try_sign()?;
                        let id = transaction.try_submit(self.wallet().rpc()).await?;
                        if let Some(notifier) = notifier.as_ref() {
                            notifier(index, balance, Some(id));
                        }
                        yield_executor().await;
                    }
                } else {
                    if let Some(notifier) = notifier.as_ref() {
                        notifier(index, aggregate_balance, None);
                    }
                    yield_executor().await;
                }
            }

            if index > last_notification + 1_000 {
                last_notification = index;
                if let Some(notifier) = notifier.as_ref() {
                    notifier(index, aggregate_balance, None);
                }
                yield_executor().await;
            }
        }

        Ok(())
    }

    // async fn receive_address(&self) -> Result<Address> {
    //     self.derivation().receive_address_manager().current_address().await
    // }

    // async fn change_address(&self) -> Result<Address> {
    //     self.derivation().change_address_manager().current_address().await
    // }

    async fn new_receive_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().receive_address_manager().new_address().await?;
        self.utxo_context().register_addresses(&[address.clone()]).await?;
        Ok(address)
    }

    async fn new_change_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().change_address_manager().new_address().await?;
        self.utxo_context().register_addresses(&[address.clone()]).await?;
        Ok(address)
    }

    fn cosigner_index(&self) -> u32 {
        0
    }

    fn create_private_keys<'l>(
        &self,
        keydata: &PrvKeyData,
        payment_secret: &Option<Secret>,
        receive: &[(&'l Address, u32)],
        change: &[(&'l Address, u32)],
    ) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
        let account_index = self.account_index();
        let cosigner_index = self.cosigner_index();

        let payload = keydata.payload.decrypt(payment_secret.as_ref())?;
        let xkey = payload.get_xprv(payment_secret.as_ref())?;

        let paths = build_derivate_paths(self.account_kind(), account_index, cosigner_index)?;
        let receive_xkey = xkey.clone().derive_path(paths.0)?;
        let change_xkey = xkey.derive_path(paths.1)?;

        let mut private_keys = vec![];
        for (address, index) in receive.iter() {
            private_keys.push((*address, *receive_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
        }
        for (address, index) in change.iter() {
            private_keys.push((*address, *change_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
        }

        Ok(private_keys)
    }
}

downcast_sync!(dyn DerivationCapableAccount);

// Wallet `Account` data structure. An account is typically a single
// HD-key derivation (derived from a wallet or from an an external secret)
// pub struct DeprecatedAccount {
//     inner: Arc<Inner>,
//     // pub account_kind: AccountKind,
//     // pub account_index: u64,
//     // pub cosigner_index: u32,
//     // pub prv_key_data_id: PrvKeyDataId,
//     // pub ecdsa: bool,
//     // pub derivation: Arc<AddressDerivationManager>,
// }

// impl DeprecatedAccount {
//     pub async fn try_new_from_storage(
//         wallet: &Arc<Wallet>,
//         name: Option<&str>,
//         title: Option<&str>,
//         storage_account_data : storage::AccountData,
//         // account_kind: AccountKind,
//         // account_index: u64,
//         // prv_key_data_id: PrvKeyDataId,
//         // pub_key_data: PubKeyData,
//         // ecdsa: bool,
//     ) -> Result<Self> {
//         // let minimum_signatures = pub_key_data.minimum_signatures.unwrap_or(1) as usize;
//         // let derivation =
//         //     AddressDerivationManager::new(wallet, account_kind, &pub_key_data, ecdsa, minimum_signatures, None, None).await?;

//         let id = AccountId::from_storage_data(&storage_account_data);
//         let data = AccountData::new_from_storage_data(&storage_account_data, wallet).await?;

//         // let id = AccountId::new(&prv_key_data_id, ecdsa, &account_kind, account_index);

//         // let id = data.id();
//         let stored = storage::Account::new(
//             id,
//             name.map(String::from),
//             title.map(String::from),
//             false,
//             storage_account_data,
//             // account_kind,
//             // account_index,
//             // false,
//             // pub_key_data.clone(),
//             // prv_key_data_id,
//             // ecdsa,
//             // pub_key_data.minimum_signatures.unwrap_or(1),
//             // pub_key_data.cosigner_index.unwrap_or(0),
//         );

//         let utxo_context = UtxoContext::new(wallet.utxo_processor(), UtxoContextBinding::AccountId(id));

//         let context = Context { listener_id: None, name : name.map(String::from), title : title.map(String::from) };
//         let inner = Inner { context : Mutex::new(context),
//             id,
//             wallet: wallet.clone(),
//             utxo_context: utxo_context.clone(),
//             // is_connected: AtomicBool::new(false),
//             // inner: Arc::new(Mutex::new(inner)),
//             // data,
//         };
//         // let id = data.id();
//         // let id = AccountId::new(&prv_key_data_id, ecdsa, &account_kind, account_index);
//         let account = DeprecatedAccount {
//             inner : Arc::new(inner),
//             // account_kind,
//             // account_index,
//             // cosigner_index: pub_key_data.cosigner_index.unwrap_or(0),
//             // prv_key_data_id,
//             // ecdsa: false,
//             // derivation,
//         };

//         Ok(account)
//     }

// pub async fn try_new_arc_from_storage(wallet: &Arc<Wallet>, stored: &storage::Account) -> Result<Arc<Self>> {
//     // let minimum_signatures = stored.pub_key_data.minimum_signatures.unwrap_or(1) as usize;
//     // let derivation = AddressDerivationManager::new(
//     //     wallet,
//     //     stored.account_kind,
//     //     &stored.pub_key_data,
//     //     stored.ecdsa,
//     //     minimum_signatures,
//     //     None,
//     //     None,
//     // )
//     // .await?;

//     let inner = Inner { listener_id: None, stored: stored.clone() };
//     let id = AccountId::new(&stored.prv_key_data_id, stored.ecdsa, &stored.account_kind, stored.account_index);
//     let utxo_context = UtxoContext::new(wallet.utxo_processor(), UtxoContextBinding::AccountId(id));
//     let account = Arc::new(Account {
//         id,
//         wallet: wallet.clone(),
//         utxo_context: utxo_context.clone(),
//         is_connected: AtomicBool::new(false),
//         inner: Arc::new(Mutex::new(inner)),
//         account_kind: stored.account_kind,
//         account_index: stored.account_index,
//         cosigner_index: stored.cosigner_index,
//         prv_key_data_id: stored.prv_key_data_id,
//         ecdsa: stored.ecdsa,
//         derivation,
//     });

//     Ok(account)
// }

// pub fn inner(&self) -> &Inner {
//     &self.inner
// }

// pub fn context(&self) -> MutexGuard<Context> {
//     self.inner.context.lock().unwrap()
// }

// pub fn id(&self) -> &AccountId {
//     &self.inner.id
// }

// pub fn wallet(&self) -> &Arc<Wallet> {
//     &self.inner.wallet
// }

// pub fn utxo_context(&self) -> &UtxoContext {
//     &self.inner.utxo_context
// }

// pub fn is_connected(&self) -> bool {
//     self.inner.is_connected.load(std::sync::atomic::Ordering::SeqCst)
// }

// pub fn name(&self) -> Option<String> {
//     self.inner.context.lock().unwrap().stored.and_then(|stored|stored.name.clone())
// }

// pub fn title(&self) -> Option<String> {
//     self.inner.context.lock().unwrap().stored.and_then(|stored|stored.title.clone())
// }

// pub fn name_or_id(&self) -> String {
//     if let Some(name) = self.name() {
//         // compensate for an empty name
//         if name.is_empty() {
//             self.inner.id.short()
//         } else {
//             name
//         }
//     } else {
//         self.inner.id.short()
//     }
// }

// pub fn name_with_id(&self) -> String {
//     if let Some(name) = self.name() {
//         // compensate for an empty name
//         if name.is_empty() {
//             self.inner.id.short()
//         } else {
//             format!("{name} {}", self.id().short())
//         }
//     } else {
//         self.inner.id.short()
//     }
// }

// pub async fn rename(&self, secret: Secret, name: Option<&str>) -> Result<()> {
//     let stored = {
//         let context = self.context();
//         let mut stored = context.stored.clone().ok_or(Error::ResidentAccount)?;
//         stored.name = name.map(String::from);
//         stored
//     };

//     self.wallet().store().as_account_store()?.store(&[&stored]).await?;

//     let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(secret));
//     self.wallet().store().commit(&ctx).await?;
//     Ok(())
// }

// pub fn balance(&self) -> Option<Balance> {
//     self.utxo_context().balance()
// }

// pub fn balance_as_strings(&self, padding: Option<usize>) -> Result<BalanceStrings> {
//     Ok(BalanceStrings::from((&self.balance(), &self.wallet().network_id()?.into(), padding)))
// }

// pub fn get_list_string(&self) -> Result<String> {
//     let name = style(self.name_with_id()).blue();
//     let balance = self.balance_as_strings(None)?;
//     let mature_utxo_size = self.utxo_context().mature_utxo_size();
//     let pending_utxo_size = self.utxo_context().pending_utxo_size();
//     let info = match (mature_utxo_size, pending_utxo_size) {
//         (0, 0) => "".to_string(),
//         (_, 0) => {
//             format!("{} UTXOs", mature_utxo_size.separated_string())
//         }
//         (0, _) => {
//             format!("{} UTXOs pending", pending_utxo_size.separated_string())
//         }
//         _ => {
//             format!("{} UTXOs, {} UTXOs pending", mature_utxo_size.separated_string(), pending_utxo_size.separated_string())
//         }
//     };
//     Ok(format!("{name}: {balance}   {}", style(info).dim()))
// }

// pub fn data(&self) -> &AccountData {
//     &self.inner.data
// }

// pub async fn prv_key_data(&self, wallet_secret : Secret) -> Result<PrvKeyData> {

//     let prv_key_data_id = self.data().prv_key_data_id();

//     let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
//     let keydata = self
//         .wallet()
//         .store()
//         .as_prv_key_data_store()?
//         .load_key_data(&access_ctx, prv_key_data_id)
//         .await?
//         .ok_or(Error::PrivateKeyNotFound(prv_key_data_id.to_hex()))?;
//     Ok(keydata)
// }

// Custom function for scanning address derivation chains

// pub(crate) fn create_private_keys<'l>(
//     &self,
//     keydata: &PrvKeyData,
//     payment_secret: &Option<Secret>,
//     receive: &[(&'l Address, u32)],
//     change: &[(&'l Address, u32)],
// ) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
//     let payload = keydata.payload.decrypt(payment_secret.as_ref())?;
//     let xkey = payload.get_xprv(payment_secret.as_ref())?;

//     let cosigner_index = self.cosigner_index;
//     let paths = build_derivate_paths(self.account_kind, self.account_index, cosigner_index)?;
//     let receive_xkey = xkey.clone().derive_path(paths.0)?;
//     let change_xkey = xkey.derive_path(paths.1)?;

//     let mut private_keys = vec![];
//     for (address, index) in receive.iter() {
//         private_keys.push((*address, *receive_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
//     }
//     for (address, index) in change.iter() {
//         private_keys.push((*address, *change_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
//     }

//     Ok(private_keys)
// }

// pub async fn receive_address(&self) -> Result<Address> {
//     self.receive_address_manager()?.current_address().await
// }

// pub async fn change_address(&self) -> Result<Address> {
//     self.change_address_manager()?.current_address().await
// }

// pub fn receive_address_manager(&self) -> Result<Arc<AddressManager>> {
//     Ok(self.derivation.receive_address_manager())
// }

// pub fn change_address_manager(&self) -> Result<Arc<AddressManager>> {
//     Ok(self.derivation.change_address_manager())
// }

// pub async fn new_receive_address(self: &Arc<Self>) -> Result<String> {
//     let address = self.receive_address_manager()?.new_address().await?;
//     self.utxo_context().register_addresses(&[address.clone()]).await?;
//     Ok(address.into())
// }

// pub async fn new_change_address(self: &Arc<Self>) -> Result<String> {
//     let address = self.change_address_manager()?.new_address().await?;
//     self.utxo_context().register_addresses(&[address.clone()]).await?;
//     Ok(address.into())
// }

// pub async fn sign(&self) -> Result<()> {
//     Ok(())
// }

// pub async fn create_unsigned_transaction(&self) -> Result<()> {
//     Ok(())
// }

// -

// /// Start Account service task
// pub async fn start(self: &Arc<Self>) -> Result<()> {
//     // if self.wallet.is_connected() {
//     self.connect().await?;
//     // }

//     Ok(())
// }

// /// Stop Account service task
// pub async fn stop(self: &Arc<Self>) -> Result<()> {
//     self.utxo_context().clear().await?;
//     self.disconnect().await?;
//     Ok(())
// }

// /// handle connection event
// pub async fn connect(self: &Arc<Self>) -> Result<()> {
//     let vacated = self.wallet().active_accounts().insert(self.clone());
//     if vacated.is_none() && self.wallet().is_connected() {
//         self.scan(None, None).await?;
//     }
//     Ok(())
// }

// /// handle disconnection event
// pub async fn disconnect(&self) -> Result<()> {
//     self.wallet().active_accounts().remove(self.id());
//     Ok(())
// }

// pub async fn sweep(
//     self: &Arc<Self>,
//     wallet_secret: Secret,
//     payment_secret: Option<Secret>,
//     abortable: &Abortable,
//     notifier: Option<GenerationNotifier>,
// ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
//     let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
//     let keydata = self
//         .wallet()
//         .store()
//         .as_prv_key_data_store()?
//         .load_key_data(&access_ctx, &self.prv_key_data_id)
//         .await?
//         .ok_or(Error::PrivateKeyNotFound(self.prv_key_data_id.to_hex()))?;
//     let signer = Arc::new(Signer::new(self, keydata, payment_secret));

//     let settings = GeneratorSettings::try_new_with_account(self, PaymentDestination::Change, Fees::None, None).await?;

//     let generator = Generator::new(settings, Some(signer), abortable);

//     let mut stream = generator.stream();
//     let mut ids = vec![];
//     while let Some(transaction) = stream.try_next().await? {
//         if let Some(notifier) = notifier.as_ref() {
//             notifier(&transaction);
//         }

//         transaction.try_sign()?;
//         transaction.log().await?;
//         let id = transaction.try_submit(self.wallet.rpc()).await?;
//         ids.push(id);
//         yield_executor().await;
//     }

//     Ok((generator.summary(), ids))
// }

// pub async fn send(
//     self: &Arc<Self>,
//     destination: PaymentDestination,
//     priority_fee_sompi: Fees,
//     payload: Option<Vec<u8>>,
//     wallet_secret: Secret,
//     payment_secret: Option<Secret>,
//     abortable: &Abortable,
//     notifier: Option<GenerationNotifier>,
// ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
//     let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
//     let keydata = self
//         .wallet
//         .store()
//         .as_prv_key_data_store()?
//         .load_key_data(&access_ctx, &self.prv_key_data_id)
//         .await?
//         .ok_or(Error::PrivateKeyNotFound(self.prv_key_data_id.to_hex()))?;
//     let signer = Arc::new(Signer::new(self, keydata, payment_secret));

//     let settings = GeneratorSettings::try_new_with_account(self, destination, priority_fee_sompi, payload).await?;

//     let generator = Generator::new(settings, Some(signer), abortable);

//     let mut stream = generator.stream();
//     let mut ids = vec![];
//     while let Some(transaction) = stream.try_next().await? {
//         if let Some(notifier) = notifier.as_ref() {
//             notifier(&transaction);
//         }

//         transaction.try_sign()?;
//         transaction.log().await?;
//         let id = transaction.try_submit(self.wallet.rpc()).await?;
//         ids.push(id);
//         yield_executor().await;
//     }

//     Ok((generator.summary(), ids))
// }

// pub async fn estimate(
//     &self,
//     destination: PaymentDestination,
//     priority_fee_sompi: Fees,
//     payload: Option<Vec<u8>>,
//     abortable: &Abortable,
// ) -> Result<GeneratorSummary> {
//     let settings = GeneratorSettings::try_new_with_account(self, destination, priority_fee_sompi, payload).await?;

//     let generator = Generator::new(settings, None, abortable);

//     let mut stream = generator.stream();
//     while let Some(_transaction) = stream.try_next().await? {
//         _transaction.log().await?;
//         yield_executor().await;
//     }

//     Ok(generator.summary())
// }
// }
