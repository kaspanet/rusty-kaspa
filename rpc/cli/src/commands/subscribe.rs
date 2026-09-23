//! `subscribe` command: stream node notifications to stdout until Ctrl-C.
//!
//! A notification channel is registered with the node, a single scope is
//! started, and incoming notifications are printed one per line (compact JSON
//! in json mode, a concise human line in text mode), flushing each line so the
//! output is pipe-friendly. On Ctrl-C the scope is stopped and the listener is
//! unregistered for a clean shutdown.

use crate::error::{CliError, Result};
use crate::output::OutputFormat;
use kaspa_addresses::Address;
use kaspa_notify::scope::{
    BlockAddedScope, FinalityConflictResolvedScope, FinalityConflictScope, NewBlockTemplateScope, PruningPointUtxoSetOverrideScope,
    Scope, SinkBlueScoreChangedScope, UtxosChangedScope, VirtualChainChangedScope, VirtualDaaScoreChangedScope,
};
use kaspa_rpc_core::Notification;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::notify::connection::{ChannelConnection, ChannelType};
use std::io::Write;
use std::sync::Arc;

/// Subscribe to a notification scope and stream events until Ctrl-C.
#[derive(clap::Args, Debug)]
pub struct Subscribe {
    #[command(subcommand)]
    scope: SubscribeScope,
}

/// The notification scope to subscribe to.
#[derive(clap::Subcommand, Debug)]
pub enum SubscribeScope {
    /// Every block added to the DAG.
    BlockAdded,
    /// Changes to the virtual selected-parent chain.
    VirtualChainChanged {
        /// Include accepted transaction ids in each notification.
        #[arg(long)]
        include_accepted_transaction_ids: bool,
    },
    /// UTXO set changes (no addresses subscribes to all).
    UtxosChanged {
        /// Filter to these addresses; repeatable. Empty subscribes to all.
        #[arg(long = "address")]
        addresses: Vec<String>,
    },
    /// Changes to the sink's blue score.
    SinkBlueScoreChanged,
    /// Changes to the virtual's DAA score.
    VirtualDaaScoreChanged,
    /// A block violating finality.
    FinalityConflict,
    /// Resolution of a finality conflict.
    FinalityConflictResolved,
    /// Pruning-point UTXO-set override events.
    PruningPointUtxoSetOverride,
    /// New block templates for miners.
    NewBlockTemplate,
}

impl Subscribe {
    /// Build the node-side [`Scope`] for the selected subcommand.
    fn to_scope(&self) -> Result<Scope> {
        let scope = match &self.scope {
            SubscribeScope::BlockAdded => Scope::BlockAdded(BlockAddedScope {}),
            SubscribeScope::VirtualChainChanged { include_accepted_transaction_ids } => {
                Scope::VirtualChainChanged(VirtualChainChangedScope::new(*include_accepted_transaction_ids))
            }
            SubscribeScope::UtxosChanged { addresses } => {
                let parsed = addresses
                    .iter()
                    .map(|a| Address::try_from(a.as_str()).map_err(|e| CliError::Usage(format!("invalid address '{a}': {e}"))))
                    .collect::<Result<Vec<_>>>()?;
                Scope::UtxosChanged(UtxosChangedScope::new(parsed))
            }
            SubscribeScope::SinkBlueScoreChanged => Scope::SinkBlueScoreChanged(SinkBlueScoreChangedScope {}),
            SubscribeScope::VirtualDaaScoreChanged => Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {}),
            SubscribeScope::FinalityConflict => Scope::FinalityConflict(FinalityConflictScope {}),
            SubscribeScope::FinalityConflictResolved => Scope::FinalityConflictResolved(FinalityConflictResolvedScope {}),
            SubscribeScope::PruningPointUtxoSetOverride => Scope::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideScope {}),
            SubscribeScope::NewBlockTemplate => Scope::NewBlockTemplate(NewBlockTemplateScope {}),
        };
        Ok(scope)
    }

    /// Register a listener, start the scope, and drain notifications to stdout
    /// until Ctrl-C, then stop the scope and unregister the listener.
    pub async fn run(&self, client: &Arc<dyn RpcApi>, format: OutputFormat) -> Result<()> {
        let scope = self.to_scope()?;

        let (sender, receiver) = async_channel::unbounded::<Notification>();
        let connection = ChannelConnection::new("kaspa-rpc-cli-subscribe", sender, ChannelType::Persistent);
        let listener_id = client.register_new_listener(connection);
        client.start_notify(listener_id, scope.clone()).await?;

        let mut stdout = std::io::stdout();
        loop {
            tokio::select! {
                biased;
                _ = tokio::signal::ctrl_c() => break,
                received = receiver.recv() => match received {
                    Ok(notification) => {
                        let line = match format {
                            OutputFormat::Json => serde_json::to_string(&notification)?,
                            OutputFormat::Text => notification.to_string(),
                        };
                        writeln!(stdout, "{line}")?;
                        stdout.flush()?;
                    }
                    // The channel only closes on shutdown of the notification subsystem.
                    Err(_) => break,
                },
            }
        }

        // Best-effort clean shutdown; the connection may already be gone.
        let _ = client.stop_notify(listener_id, scope).await;
        let _ = client.unregister_listener(listener_id).await;
        Ok(())
    }
}
