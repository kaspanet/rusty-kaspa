use futures_util::{select, FutureExt};
use kaspa_wallet_core::{
    api::WalletApi,
    events::Events,
    prelude::{AccountDescriptor, Address},
    wallet::Wallet,
};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

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
                                        if sync_state.is_synced() {
                                            if let Err(err) = wallet.clone().wallet_reload(false).await {
                                                panic!("Wallet reloading failed: {}", err)
                                            }
                                        }
                                    },
                                    Events::Balance { balance: _new_balance, .. } => {
                                        // TBD: index balance per address for call
                                    },
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        });

        Service { wallet, shutdown_sender: Arc::new(Mutex::new(Some(shutdown_sender))), ecdsa }
    }

    pub fn receive_addresses(&self) -> Vec<Address> {
        // TODO: move into WalletApi
        let manager = self.wallet.account().unwrap().as_derivation_capable().unwrap().derivation().receive_address_manager();
        manager.get_range_with_args(0..manager.index(), false).unwrap()
    }

    pub fn wallet(&self) -> Arc<Wallet> {
        self.wallet.clone()
    }

    pub fn descriptor(&self) -> AccountDescriptor {
        self.wallet.account().unwrap().descriptor().unwrap()
    }

    pub fn initiate_shutdown(&self) {
        let mut sender = self.shutdown_sender.lock().unwrap();
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
}
