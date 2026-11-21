use crate::policy::{self, Balances, ChangeAddressSource, GenerationParams, UsedOutpoints};
use fee_policy::FeePolicy;
use futures_util::{FutureExt, select};
use kaspa_addresses::Prefix;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_rpc_core::{RpcTransaction, RpcTransactionId};
use kaspa_wallet_core::account::Account;
use kaspa_wallet_core::account::variants::bip32::{BIP32_ACCOUNT_KIND, Payload};
use kaspa_wallet_core::api::NewAddressKind;
use kaspa_wallet_core::derivation::AddressManager;
use kaspa_wallet_core::error::Error;
use kaspa_wallet_core::tx::PendingTransaction;
use kaspa_wallet_core::utxo::UtxoEntryReference;
use kaspa_wallet_core::{
    api::WalletApi,
    events::Events,
    prelude::{AccountDescriptor, Address},
    wallet::Wallet,
};
use kaspa_wallet_grpc_core::kaspawalletd;
use kaspa_wallet_grpc_core::kaspawalletd::{CreateUnsignedTransactionsRequest, UtxosByAddressesEntry, fee_policy};
use kaspa_wallet_grpc_core::protoserialization::PartiallySignedTransaction;
use kaspa_wallet_grpc_core::pst;
use log::{error, info, warn};
use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard, oneshot};
use tonic::Status;

const RELOAD_RETRY_DELAY: Duration = Duration::from_secs(2);

pub struct Service {
    wallet: Arc<Wallet>,
    shutdown_sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    // TODO: Extend the partially serialized transaction or transaction structure with a boolean field 'ecdsa'
    ecdsa: bool,
    lock: Arc<RwLock<()>>,
    ready: Arc<AtomicBool>,
    used_outpoints: Mutex<UsedOutpoints>,
}

impl Service {
    pub fn with_notification_pipe_task(wallet: Arc<Wallet>, shutdown_sender: oneshot::Sender<()>, ecdsa: bool) -> Self {
        let channel = wallet.multiplexer().channel();
        let lock = Arc::new(RwLock::new(()));
        let ready = Arc::new(AtomicBool::new(true));

        tokio::spawn({
            let wallet = wallet.clone();
            let lock = lock.clone();
            let ready = ready.clone();

            async move {
                loop {
                    select! {
                        msg = channel.receiver.recv().fuse() => {
                            if let Ok(msg) = msg {
                                match *msg {
                                    Events::SyncState { sync_state } if sync_state.is_synced() => {
                                        Self::reload_wallet(&wallet, &lock, &ready).await;
                                    },
                                    Events::UtxoProcStart => {
                                        Self::reload_wallet(&wallet, &lock, &ready).await;
                                    },
                                    Events::Connect { ref url, .. } => {
                                        info!("Connected to node {}", url.as_deref().unwrap_or("(unknown url)"));
                                    },
                                    Events::Disconnect { ref url, .. } => {
                                        warn!("Disconnected from node {}", url.as_deref().unwrap_or("(unknown url)"));
                                    },
                                    _ => {}
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        });

        Service {
            wallet,
            shutdown_sender: Arc::new(Mutex::new(Some(shutdown_sender))),
            ecdsa,
            lock,
            ready,
            used_outpoints: Mutex::new(UsedOutpoints::default()),
        }
    }

    async fn reload_wallet(wallet: &Arc<Wallet>, lock: &RwLock<()>, ready: &AtomicBool) {
        {
            let _guard = lock.write().await;
            ready.store(false, Ordering::SeqCst);
        }
        loop {
            match wallet.clone().wallet_reload(true).await {
                Ok(()) => {
                    ready.store(true, Ordering::SeqCst);
                    return;
                }
                Err(err) => error!("Wallet reloading failed: {err}"),
            }
            if !wallet.is_connected() {
                warn!("Node connection lost, wallet reload will be retried after reconnection");
                return;
            }
            tokio::time::sleep(RELOAD_RETRY_DELAY).await;
            if !wallet.is_connected() {
                warn!("Node connection lost, wallet reload will be retried after reconnection");
                return;
            }
        }
    }

    pub async fn read_guard(&self) -> RwLockReadGuard<'_, ()> {
        self.lock.read().await
    }

    pub async fn write_guard(&self) -> RwLockWriteGuard<'_, ()> {
        self.lock.write().await
    }

    fn used_outpoints(&self) -> MutexGuard<'_, UsedOutpoints> {
        self.used_outpoints.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn ensure_synced(&self) -> Result<(), Status> {
        if !self.ready.load(Ordering::SeqCst) {
            return Err(Status::unavailable("wallet daemon is not synced yet"));
        }
        let account = self.wallet.account().map_err(|err| Status::failed_precondition(err.to_string()))?;
        let has_balance = account.descriptor().is_ok_and(|descriptor| descriptor.balance.is_some());
        if self.wallet.is_connected() && self.wallet.is_synced() && has_balance {
            Ok(())
        } else {
            Err(Status::unavailable("wallet daemon is not synced yet"))
        }
    }

    pub async fn calculate_fee_limits(&self, fee_policy: Option<kaspawalletd::FeePolicy>) -> Result<(f64, u64), Status> {
        let fee_policy = fee_policy.and_then(|fee_policy| fee_policy.fee_policy);
        let fees: (f64, u64) = if let Some(policy) = fee_policy {
            match policy {
                FeePolicy::MaxFeeRate(max_fee_rate) => {
                    let max_fee_rate = policy::validate_fee_rate(max_fee_rate, "max fee rate")?;
                    let fee_rate = max_fee_rate.min(self.normal_fee_rate().await?);
                    (fee_rate, u64::MAX)
                }
                FeePolicy::ExactFeeRate(exact_fee_rate) => (policy::validate_fee_rate(exact_fee_rate, "fee rate")?, u64::MAX),
                FeePolicy::MaxFee(max_fee) => (self.normal_fee_rate().await?, max_fee),
            }
        } else {
            (self.normal_fee_rate().await?, SOMPI_PER_KASPA)
        };
        Ok(fees)
    }

    async fn normal_fee_rate(&self) -> Result<f64, Status> {
        let estimate = self.wallet.rpc_api().get_fee_estimate().await.map_err(|err| Status::unavailable(err.to_string()))?;
        let fee_rate = estimate
            .normal_buckets
            .first()
            .map(|bucket| bucket.feerate)
            .ok_or_else(|| Status::unavailable("missing normal feerate bucket"))?;
        policy::node_fee_rate(fee_rate)
    }

    pub fn receive_addresses(&self) -> Result<Vec<Address>, Status> {
        // TODO: move into WalletApi
        let (receive, _) = self.address_managers()?;
        Self::addresses_through_current_index(&receive, "receive")
    }

    pub fn wallet_addresses(&self) -> Result<Vec<Address>, Status> {
        let (receive, change) = self.address_managers()?;
        let mut addresses = Self::addresses_through_current_index(&receive, "receive")?;
        addresses.extend(Self::addresses_through_current_index(&change, "change")?);
        Ok(addresses)
    }

    pub fn tracked_addresses(&self) -> Result<Vec<Address>, Status> {
        let account = self.wallet.account().map_err(|err| Status::failed_precondition(err.to_string()))?;
        let registered = account.utxo_context().addresses().iter().map(|address| address.as_ref().clone()).collect::<Vec<_>>();
        Ok(policy::merge_addresses(self.wallet_addresses()?, registered))
    }

    pub async fn ensure_payment_secret_not_required(&self) -> Result<(), Status> {
        let account = self.wallet.account().map_err(|err| Status::failed_precondition(err.to_string()))?;
        let prv_key_data_id = account.prv_key_data_id().map_err(|err| Status::failed_precondition(err.to_string()))?;
        let info = self
            .wallet
            .store()
            .as_prv_key_data_store()
            .map_err(|err| Status::internal(err.to_string()))?
            .load_key_info(prv_key_data_id)
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .ok_or_else(|| Status::internal(format!("private key data {prv_key_data_id} not found")))?;
        if info.is_encrypted() {
            return Err(Status::failed_precondition("wallets protected with a payment secret are not supported"));
        }
        Ok(())
    }

    fn address_managers(&self) -> Result<(Arc<AddressManager>, Arc<AddressManager>), Status> {
        let derivation = self
            .wallet
            .account()
            .map_err(|err| Status::failed_precondition(err.to_string()))?
            .as_derivation_capable()
            .map_err(|err| Status::failed_precondition(err.to_string()))?
            .derivation();
        Ok((derivation.receive_address_manager(), derivation.change_address_manager()))
    }

    fn addresses_through_current_index(manager: &AddressManager, kind: &str) -> Result<Vec<Address>, Status> {
        let end_exclusive =
            manager.index().checked_add(1).ok_or_else(|| Status::internal(format!("{kind} address index overflow")))?;
        manager.get_range_with_args(0..end_exclusive, false).map_err(|err| Status::internal(err.to_string()))
    }

    pub fn wallet(&self) -> Arc<Wallet> {
        self.wallet.clone()
    }

    pub fn descriptor(&self) -> Result<AccountDescriptor, Status> {
        self.wallet
            .account()
            .map_err(|err| Status::failed_precondition(err.to_string()))?
            .descriptor()
            .map_err(|err| Status::internal(err.to_string()))
    }

    pub fn initiate_shutdown(&self) {
        let mut sender = self.shutdown_sender.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(shutdown_sender) = sender.take() {
            let _ = shutdown_sender.send(());
        }
    }

    /// Returns whether the service should use ECDSA signatures instead of Schnorr signatures.
    /// This flag is used when processing transactions to determine the appropriate signature scheme.
    /// Currently set via command-line arguments, but this is temporary - the signature scheme
    /// should be determined per transaction by extending the partially serialized transaction
    /// or transaction structure with this field.
    pub fn use_ecdsa(&self) -> bool {
        self.ecdsa
    }

    pub async fn create_unsigned_transactions(
        &self,
        request: CreateUnsignedTransactionsRequest,
    ) -> Result<Vec<PartiallySignedTransaction>, Status> {
        let (account, payload) = self.single_signature_bip32_account()?;
        self.ensure_synced()?;

        let CreateUnsignedTransactionsRequest { address, amount, from, use_existing_change_address, is_send_all, fee_policy } =
            request;
        policy::validate_payment_amount(amount, is_send_all)?;
        let (fee_rate, max_fee) = self.calculate_fee_limits(fee_policy).await?;
        let to = Address::try_from(address).map_err(|err| Status::invalid_argument(err.to_string()))?;
        let current_network = self.wallet.network_id().map_err(|err| Status::internal(err.to_string()))?;
        if to.prefix != Prefix::from(current_network) {
            return Err(Status::invalid_argument(format!(
                "decoded address is of wrong network. Expected {} but got {}",
                Prefix::from(current_network),
                to.prefix
            )));
        }
        let from_addresses = from
            .iter()
            .map(|address| Address::try_from(address.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| Status::invalid_argument(err.to_string()))?;

        info!("Processing request for account_id: {}", self.descriptor()?.account_id);

        let addresses = self.tracked_addresses()?;
        if let Some(non_existent_address) = from_addresses.iter().find(|from| addresses.iter().all(|address| &address != from)) {
            return Err(Status::invalid_argument(format!("specified from address {non_existent_address} does not exists")));
        }

        // If specific addresses for sending are specified, use them
        // Otherwise, use all wallet addresses to search for UTXO
        let search_addresses = if from_addresses.is_empty() {
            info!("No specific addresses specified, searching UTXOs in all wallet addresses");
            None // Search UTXO from all addresses in wallet
        } else {
            info!("Searching UTXOs in specified addresses: {:?}", from_addresses);
            Some(from_addresses.clone())
        };

        let mempool_addresses = search_addresses.clone().unwrap_or(addresses);
        let mempool_spent = self.mempool_spent_outpoints(mempool_addresses).await?;
        let utxos = account.clone().get_utxos(search_addresses, None).await.map_err(|err| Status::internal(err.to_string()))?;
        let coinbase_maturity = policy::coinbase_maturity(current_network)?;
        let current_daa_score =
            self.wallet.current_daa_score().ok_or_else(|| Status::unavailable("current DAA score is not available yet"))?;
        let utxos = policy::spendable_utxos(
            utxos.into_iter().map(|utxo| UtxoEntryReference { utxo: Arc::new(utxo) }).collect(),
            coinbase_maturity,
            current_daa_score,
        );
        let now = Instant::now();
        let utxos = {
            let mut used_outpoints = self.used_outpoints();
            used_outpoints.prune(now);
            utxos
                .into_iter()
                .filter(|utxo| {
                    let outpoint = TransactionOutpoint::from(&utxo.utxo.outpoint);
                    !mempool_spent.contains(&outpoint) && !used_outpoints.is_used(&outpoint, now)
                })
                .collect::<Vec<_>>()
        };

        // Sort UTXOs by amount descending to optimize transaction weight
        // Use large UTXOs in priority to minimize the number of inputs
        let mut sorted_utxos = utxos;
        sorted_utxos.sort_unstable_by_key(|a| Reverse(a.utxo.amount));

        if sorted_utxos.is_empty() {
            return Err(Status::failed_precondition("couldn't find funds to spend"));
        }

        let change_address = match policy::change_address_source(use_existing_change_address, &from_addresses) {
            ChangeAddressSource::FirstFrom(address) => address,
            ChangeAddressSource::Current => {
                self.descriptor()?.change_address.ok_or(Status::internal("change address doesn't exist"))?
            }
            ChangeAddressSource::New => {
                self.wallet()
                    .accounts_create_new_address(*account.id(), NewAddressKind::Change)
                    .await
                    .map_err(|err| Status::internal(err.to_string()))?
                    .address
            }
        };

        let total_balance = sorted_utxos.iter().fold(0u64, |total, utxo| total.saturating_add(utxo.utxo.amount));
        info!("Found {} UTXOs with total value {} sompi", sorted_utxos.len(), total_balance);

        let generation = GenerationParams {
            network_id: current_network,
            utxos: sorted_utxos,
            change_address: &change_address,
            sig_op_count: account.sig_op_count(),
            minimum_signatures: account.minimum_signatures(),
            to: &to,
            amount,
            is_send_all,
        };
        let transactions = policy::generate_with_max_fee(&generation, fee_rate, max_fee)?;
        self.unsigned_partially_signed_transactions(&account, &payload, transactions)
    }

    pub async fn submit_transaction(&self, transaction: RpcTransaction) -> Result<RpcTransactionId, Status> {
        let outpoints = Self::input_outpoints(&transaction);
        let transaction_id =
            self.wallet.rpc_api().submit_transaction(transaction, false).await.map_err(|err| Status::internal(err.to_string()))?;
        self.used_outpoints().record(outpoints, Instant::now());
        Ok(transaction_id)
    }

    pub async fn submit_transaction_replacement(&self, transaction: RpcTransaction) -> Result<RpcTransactionId, Status> {
        let outpoints = Self::input_outpoints(&transaction);
        let response = self
            .wallet
            .rpc_api()
            .submit_transaction_replacement(transaction)
            .await
            .map_err(|err| Status::internal(err.to_string()))?;
        self.used_outpoints().record(outpoints, Instant::now());
        Ok(response.transaction_id)
    }

    async fn mempool_spent_outpoints(&self, addresses: Vec<Address>) -> Result<HashSet<TransactionOutpoint>, Status> {
        let mempool_entries = self
            .wallet
            .rpc_api()
            .get_mempool_entries_by_addresses(addresses, true, false)
            .await
            .map_err(|err| Status::unavailable(err.to_string()))?;
        Ok(policy::mempool_spent_outpoints(&mempool_entries))
    }

    fn input_outpoints(transaction: &RpcTransaction) -> Vec<TransactionOutpoint> {
        transaction.inputs.iter().map(|input| TransactionOutpoint::from(input.previous_outpoint)).collect()
    }

    pub async fn balances(&self) -> Result<Balances, Status> {
        self.ensure_synced()?;
        let addresses = self.tracked_addresses()?;
        let rpc = self.wallet.rpc_api();
        let mempool_entries = rpc
            .get_mempool_entries_by_addresses(addresses.clone(), true, false)
            .await
            .map_err(|err| Status::unavailable(err.to_string()))?;
        let entries = rpc.get_utxos_by_addresses(addresses).await.map_err(|err| Status::unavailable(err.to_string()))?;
        let virtual_daa_score = rpc.get_block_dag_info().await.map_err(|err| Status::unavailable(err.to_string()))?.virtual_daa_score;
        let network_id = self.wallet.network_id().map_err(|err| Status::internal(err.to_string()))?;
        let coinbase_maturity = policy::coinbase_maturity(network_id)?;
        let excluded = policy::mempool_spent_outpoints(&mempool_entries);
        Ok(policy::compute_balances(&entries, &excluded, virtual_daa_score, coinbase_maturity))
    }

    pub async fn external_spendable_utxos(&self, address: String) -> Result<Vec<UtxosByAddressesEntry>, Status> {
        let address = Address::try_from(address).map_err(|err| Status::invalid_argument(err.to_string()))?;
        let network_id = self.wallet.network_id().map_err(|err| Status::internal(err.to_string()))?;
        if address.prefix != Prefix::from(network_id) {
            return Err(Status::invalid_argument(format!(
                "decoded address is of wrong network. Expected {} but got {}",
                Prefix::from(network_id),
                address.prefix
            )));
        }
        let rpc = self.wallet.rpc_api();
        let entries = rpc.get_utxos_by_addresses(vec![address]).await.map_err(|err| Status::unavailable(err.to_string()))?;
        let fee_rate = self.normal_fee_rate().await?;
        let virtual_daa_score = rpc.get_block_dag_info().await.map_err(|err| Status::unavailable(err.to_string()))?.virtual_daa_score;
        let coinbase_maturity = policy::coinbase_maturity(network_id)?;
        let fee_per_input =
            policy::fee_per_input(policy::signed_p2pk_input_compute_mass(&Params::from(network_id.network_type)), fee_rate);
        Ok(entries
            .into_iter()
            .filter(|entry| policy::is_external_utxo_spendable(&entry.utxo_entry, virtual_daa_score, coinbase_maturity, fee_per_input))
            .map(|entry| policy::utxo_entry_wrapper(entry).into())
            .collect())
    }

    fn single_signature_bip32_account(&self) -> Result<(Arc<dyn Account>, Payload), Status> {
        let account = self.wallet.account().map_err(|err| Status::failed_precondition(err.to_string()))?;
        if account.account_kind() != BIP32_ACCOUNT_KIND || account.minimum_signatures() != 1 {
            return Err(Status::unimplemented("only bip32 single-signature accounts are supported"));
        }
        let payload = Payload::try_load(&account.to_storage().map_err(|err| Status::internal(err.to_string()))?)
            .map_err(|err| Status::internal(err.to_string()))?;
        if payload.xpub_keys.len() != 1 {
            return Err(Status::unimplemented("only bip32 single-signature accounts are supported"));
        }
        if payload.ecdsa || self.use_ecdsa() {
            return Err(Status::unimplemented("Ecdsa signing is not supported yet"));
        }
        Ok((account, payload))
    }

    fn unsigned_partially_signed_transactions(
        &self,
        account: &Arc<dyn Account>,
        payload: &Payload,
        transactions: Vec<PendingTransaction>,
    ) -> Result<Vec<PartiallySignedTransaction>, Status> {
        let account_xpub =
            payload.xpub_keys.first().ok_or_else(|| Status::unimplemented("only bip32 single-signature accounts are supported"))?;
        let network_type = self.wallet.network_id().map_err(|err| Status::internal(err.to_string()))?.network_type();
        let derivation =
            account.clone().as_derivation_capable().map_err(|err| Status::failed_precondition(err.to_string()))?.derivation();

        transactions
            .into_iter()
            .map(|transaction| {
                let rpc_transaction = transaction.rpc_transaction();
                let utxo_entries = transaction.utxo_entries();
                let inputs = rpc_transaction
                    .inputs
                    .iter()
                    .map(|input| {
                        let entry = utxo_entries
                            .values()
                            .find(|entry| {
                                entry.utxo.outpoint.transaction_id() == input.previous_outpoint.transaction_id
                                    && entry.utxo.outpoint.index() == input.previous_outpoint.index
                            })
                            .ok_or_else(|| Status::internal("missing utxo entry for transaction input"))?;
                        let address = entry.utxo.address.as_ref().ok_or_else(|| Status::internal("missing utxo entry address"))?;
                        let (receive, change) =
                            derivation.addresses_indexes(&[address]).map_err(|err| Status::internal(err.to_string()))?;
                        let derivation_path = match (receive.first(), change.first()) {
                            (Some((_, index)), _) => pst::derivation_path(false, *index),
                            (None, Some((_, index))) => pst::derivation_path(true, *index),
                            (None, None) => return Err(Status::internal(format!("address {address} does not belong to the account"))),
                        };
                        Ok(pst::InputSource {
                            amount: entry.utxo.amount,
                            script_public_key: entry.utxo.script_public_key.clone(),
                            derivation_path,
                        })
                    })
                    .collect::<Result<Vec<_>, Status>>()?;
                pst::unsigned_partially_signed_transaction(rpc_transaction, inputs, account_xpub, network_type)
            })
            .collect()
    }

    pub async fn sign_partially_signed_transactions(
        &self,
        transactions: Vec<PartiallySignedTransaction>,
        password: String,
    ) -> Result<Vec<PartiallySignedTransaction>, Status> {
        self.ensure_payment_secret_not_required().await?;
        let (account, payload) = self.single_signature_bip32_account()?;
        let network_type = self.wallet.network_id().map_err(|err| Status::internal(err.to_string()))?.network_type();
        let prv_key_data = account.prv_key_data(password.into()).await.map_err(|err| match err {
            Error::Chacha20poly1305(_) => Status::invalid_argument(err.to_string()),
            err => Status::internal(err.to_string()),
        })?;
        let master = prv_key_data.get_xprv(None).map_err(|err| Status::internal(err.to_string()))?;
        let account_xprv = pst::account_extended_private_key(master, payload.account_index)?;

        transactions
            .into_iter()
            .map(|transaction| pst::sign_partially_signed_transaction(transaction, &account_xprv, network_type))
            .collect()
    }
}
