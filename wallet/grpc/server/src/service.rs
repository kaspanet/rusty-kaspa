use fee_policy::FeePolicy;
use futures_util::{FutureExt, TryStreamExt, select};
use kaspa_addresses::Prefix;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use kaspa_wallet_core::account::Account;
use kaspa_wallet_core::account::variants::bip32::{BIP32_ACCOUNT_KIND, Payload};
use kaspa_wallet_core::api::NewAddressKind;
use kaspa_wallet_core::prelude::{PaymentDestination, PaymentOutput, PaymentOutputs};
use kaspa_wallet_core::tx::{Fees, Generator, GeneratorSettings, PendingTransaction};
use kaspa_wallet_core::utxo::UtxoEntryReference;
use kaspa_wallet_core::{
    api::WalletApi,
    events::Events,
    prelude::{AccountDescriptor, Address},
    wallet::Wallet,
};
use kaspa_wallet_grpc_core::kaspawalletd;
use kaspa_wallet_grpc_core::kaspawalletd::fee_policy;
use kaspa_wallet_grpc_core::protoserialization::PartiallySignedTransaction;
use kaspa_wallet_grpc_core::pst;
use log::{error, info};
use std::cmp::Reverse;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::oneshot;
use tonic::Status;

pub struct Service {
    wallet: Arc<Wallet>,
    shutdown_sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    // TODO: Extend the partially serialized transaction or transaction structure with a boolean field 'ecdsa'
    ecdsa: bool,
}

impl Service {
    pub fn with_notification_pipe_task(wallet: Arc<Wallet>, shutdown_sender: oneshot::Sender<()>, ecdsa: bool) -> Self {
        let channel = wallet.multiplexer().channel();

        tokio::spawn({
            let wallet = wallet.clone();

            async move {
                loop {
                    select! {
                        msg = channel.receiver.recv().fuse() => {
                            if let Ok(msg) = msg {
                                match *msg {
                                    Events::SyncState { sync_state } => {
                                        if sync_state.is_synced()
                                            && let Err(err) = wallet.clone().wallet_reload(true).await {
                                                error!("Wallet reloading failed: {}", err);
                                            }
                                    },
                                    Events::UtxoProcStart => {
                                        if let Err(err) = wallet.clone().wallet_reload(true).await {
                                            error!("Wallet reloading failed: {}", err);
                                        }
                                    },
                                    Events::Balance { balance: _new_balance, .. } => {
                                        // TBD: index balance per address for call
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

        Service { wallet, shutdown_sender: Arc::new(Mutex::new(Some(shutdown_sender))), ecdsa }
    }

    pub async fn calculate_fee_limits(&self, fee_policy: Option<kaspawalletd::FeePolicy>) -> Result<(f64, u64), Status> {
        let fee_policy = fee_policy.and_then(|fee_policy| fee_policy.fee_policy);
        const MIN_FEE_RATE: f64 = 1.0;
        let fees: (f64, u64) = if let Some(policy) = fee_policy {
            match policy {
                FeePolicy::MaxFeeRate(max_fee_rate) => {
                    if max_fee_rate < MIN_FEE_RATE {
                        return Err(Status::invalid_argument(format!(
                            "requested max fee rate {} is too low, minimum fee rate is {}",
                            max_fee_rate, MIN_FEE_RATE
                        )));
                    };
                    let fee_rate = max_fee_rate.min(self.normal_fee_rate().await?);
                    (fee_rate, u64::MAX)
                }
                FeePolicy::ExactFeeRate(exact_fee_rate) => {
                    if exact_fee_rate < MIN_FEE_RATE {
                        return Err(Status::invalid_argument(format!(
                            "requested fee rate {} is too low, minimum fee rate is {}",
                            exact_fee_rate, MIN_FEE_RATE
                        )));
                    }
                    (exact_fee_rate, u64::MAX)
                }
                FeePolicy::MaxFee(max_fee) => (self.normal_fee_rate().await?, max_fee),
            }
        } else {
            (self.normal_fee_rate().await?, SOMPI_PER_KASPA)
        };
        Ok(fees)
    }

    async fn normal_fee_rate(&self) -> Result<f64, Status> {
        let estimate = self.wallet.rpc_api().get_fee_estimate().await.map_err(|err| Status::unavailable(err.to_string()))?;
        estimate
            .normal_buckets
            .first()
            .map(|bucket| bucket.feerate)
            .ok_or_else(|| Status::unavailable("missing normal feerate bucket"))
    }

    pub fn receive_addresses(&self) -> Result<Vec<Address>, Status> {
        // TODO: move into WalletApi
        let manager = self
            .wallet
            .account()
            .map_err(|err| Status::failed_precondition(err.to_string()))?
            .as_derivation_capable()
            .map_err(|err| Status::failed_precondition(err.to_string()))?
            .derivation()
            .receive_address_manager();
        let end_exclusive = manager.index().checked_add(1).ok_or_else(|| Status::internal("receive address index overflow"))?;
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

    pub async fn unsigned_txs(
        &self,
        to: Address,
        amount: u64,
        use_existing_change_address: bool,
        is_send_all: bool,
        fee_rate: f64,
        max_fee: u64,
        from_addresses: Vec<Address>,
    ) -> Result<Vec<PendingTransaction>, Status> {
        let current_network = self.wallet().network_id().map_err(|err| Status::internal(err.to_string()))?;
        if to.prefix != Prefix::from(current_network) {
            return Err(Status::invalid_argument(format!(
                "decoded address is of wrong network. Expected {} but got {}",
                Prefix::from(current_network),
                to.prefix
            )));
        }

        let account = self.wallet().account().map_err(|err| Status::internal(err.to_string()))?;

        info!("Processing request for account_id: {}", self.descriptor()?.account_id);

        let addresses = account.account_addresses().map_err(|err| Status::internal(err.to_string()))?;
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
            Some(from_addresses)
        };

        let utxos = account.clone().get_utxos(search_addresses, None).await.map_err(|err| Status::internal(err.to_string()))?;

        // Sort UTXOs by amount descending to optimize transaction weight
        // Use large UTXOs in priority to minimize the number of inputs
        let mut sorted_utxos = utxos;
        sorted_utxos.sort_unstable_by_key(|a| Reverse(a.amount));

        let change_address = if !use_existing_change_address {
            self.wallet()
                .accounts_create_new_address(self.descriptor()?.account_id, NewAddressKind::Change)
                .await
                .map_err(|err| Status::internal(err.to_string()))?
                .address
        } else {
            self.descriptor()?.change_address.ok_or(Status::internal("change address doesn't exist"))?.clone()
        };

        let total_balance: u64 = sorted_utxos.iter().map(|utxo| utxo.amount).sum();
        let output_amount = if is_send_all { total_balance } else { amount };

        info!("Found {} UTXOs with total value {} sompi", sorted_utxos.len(), total_balance);

        let settings = GeneratorSettings::try_new_with_iterator(
            current_network,
            Box::new(sorted_utxos.into_iter().map(|utxo| UtxoEntryReference { utxo: Arc::new(utxo) })),
            None,
            change_address,
            account.sig_op_count(),
            account.minimum_signatures(),
            PaymentDestination::PaymentOutputs(PaymentOutputs { outputs: vec![PaymentOutput::new(to, output_amount)] }),
            Some(fee_rate),
            Fees::SenderPays(0), // FIXME: @zelenevn
            None,
            None,
        )
        .map_err(|err| Status::internal(err.to_string()))?;

        let generator = Generator::try_new(settings, None, None).map_err(|err| Status::internal(err.to_string()))?;

        let mut stream = generator.stream();
        let mut txs = vec![];
        while let Some(transaction) = stream.try_next().await.map_err(|err| Status::internal(err.to_string()))? {
            txs.push(transaction);
        }
        if generator.summary().aggregate_fees > max_fee {
            return Err(Status::failed_precondition(format!(
                "aggregate fees {} exceeds requested max {}",
                generator.summary().aggregate_fees,
                max_fee
            )));
        }
        Ok(txs)
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
        Ok((account, payload))
    }

    pub fn unsigned_partially_signed_transactions(
        &self,
        transactions: Vec<PendingTransaction>,
    ) -> Result<Vec<PartiallySignedTransaction>, Status> {
        let (account, payload) = self.single_signature_bip32_account()?;
        let account_xpub =
            payload.xpub_keys.first().ok_or_else(|| Status::unimplemented("only bip32 single-signature accounts are supported"))?;
        let network_type = self.wallet.network_id().map_err(|err| Status::internal(err.to_string()))?.network_type();
        let derivation = account.as_derivation_capable().map_err(|err| Status::failed_precondition(err.to_string()))?.derivation();

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
        if self.use_ecdsa() {
            return Err(Status::unimplemented("Ecdsa signing is not supported yet"));
        }

        let (account, payload) = self.single_signature_bip32_account()?;
        if payload.ecdsa {
            return Err(Status::unimplemented("Ecdsa signing is not supported yet"));
        }
        let network_type = self.wallet.network_id().map_err(|err| Status::internal(err.to_string()))?.network_type();
        let prv_key_data = account.prv_key_data(password.into()).await.map_err(|err| Status::internal(err.to_string()))?;
        let master = prv_key_data.get_xprv(None).map_err(|err| Status::internal(err.to_string()))?;
        let account_xprv = pst::account_extended_private_key(master, payload.account_index)?;

        transactions
            .into_iter()
            .map(|transaction| pst::sign_partially_signed_transaction(transaction, &account_xprv, network_type))
            .collect()
    }
}
