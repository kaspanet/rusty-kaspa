use crate::imports::*;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_wallet_core::error::Error as WalletError;
use kaspa_wallet_core::storage::Binding;
use kaspa_wallet_core::wallet::WalletGuard;
use workflow_core::time::unixtime_as_millis_u64;
#[derive(Default, Handler)]
#[help("Display transaction history")]
pub struct History;

impl History {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let guard = ctx.wallet().guard();
        let guard = guard.lock().await;

        if argv.is_empty() {
            self.display_help(ctx, argv).await?;
            return Ok(());
        }

        let account = ctx.account().await?;
        let network_id = ctx.wallet().network_id()?;
        let binding = Binding::from(&account);
        let current_daa_score = ctx.wallet().current_daa_score();

        let (last, include_utxo) = match argv.remove(0).as_str() {
            "export" => {
                return self.export_csv(ctx, argv, &guard).await;
            }
            "lookup" => {
                let transaction_id = if argv.is_empty() {
                    tprintln!(ctx, "usage: history lookup <transaction id>");
                    return Ok(());
                } else {
                    argv.remove(0)
                };

                let txid = TransactionId::from_hex(transaction_id.as_str())?;
                let store = ctx.wallet().store().as_transaction_record_store()?;
                match store.load_single(&binding, &network_id, &txid).await {
                    Ok(tx) => {
                        let lines = tx
                            .format_transaction_with_args(
                                &ctx.wallet(),
                                None,
                                current_daa_score,
                                true,
                                true,
                                Some(account.clone()),
                                &guard,
                            )
                            .await;
                        lines.iter().for_each(|line| tprintln!(ctx, "{line}"));
                    }
                    Err(_) => {
                        tprintln!(ctx, "transaction not found");
                    }
                }

                return Ok(());
            }
            "list" => {
                let last = if argv.is_empty() { None } else { argv[0].parse::<usize>().ok() };
                (last, false)
            }
            "details" => {
                let last = if argv.is_empty() { None } else { argv[0].parse::<usize>().ok() };
                (last, true)
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'");
                self.display_help(ctx, argv).await?;
                return Ok(());
            }
        };

        let store = ctx.wallet().store().as_transaction_record_store()?;
        let mut ids = match store.transaction_id_iter(&binding, &network_id).await {
            Ok(ids) => ids,
            Err(err) => {
                if matches!(err, WalletError::NoRecordsFound) {
                    tprintln!(ctx);
                    tprintln!(ctx, "No transactions found for this account.");
                    tprintln!(ctx);
                } else {
                    terrorln!(ctx, "{err}");
                }
                return Ok(());
            }
        };
        let length = ids.size_hint().0;
        let skip = if let Some(last) = last { length.saturating_sub(last) } else { 0 };
        let mut index = 0;
        let page = 25;

        tprintln!(ctx);

        while let Some(id) = ids.try_next().await? {
            if index >= skip {
                if index > 0 && index % page == 0 {
                    tprintln!(ctx);
                    let prompt = format!(
                        "Displaying transactions {} to {} of {} (press any key to continue, 'Q' to abort)",
                        index.separated_string(),
                        (index + page).separated_string(),
                        length.separated_string()
                    );
                    let query = ctx.term().kbhit(Some(&prompt)).await?;
                    tprintln!(ctx);
                    if query.to_lowercase() == "q" {
                        return Ok(());
                    }
                }

                match store.load_single(&binding, &network_id, &id).await {
                    Ok(tx) => {
                        let lines = tx
                            .format_transaction_with_args(
                                &ctx.wallet(),
                                None,
                                current_daa_score,
                                include_utxo,
                                true,
                                Some(account.clone()),
                                &guard,
                            )
                            .await;
                        lines.iter().for_each(|line| tprintln!(ctx, "{line}"));
                    }
                    Err(err) => {
                        terrorln!(ctx, "Unable to read transaction data: {err}");
                    }
                }
            }
            index += 1;
        }

        tprintln!(ctx);
        tprintln!(ctx, "{} transactions", length.separated_string());
        tprintln!(ctx);

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("list [<last N transactions>]", "List transactions"),
                ("details [<last N transactions>]", "List transactions with UTXO details"),
                ("lookup <transaction id>", "Lookup transaction in the history"),
                ("export [<filename>]", "Export transaction history to CSV file"),
            ],
            None,
        )?;

        Ok(())
    }

    async fn export_csv(
        self: Arc<Self>,
        ctx: Arc<KaspaCli>,
        mut argv: Vec<String>,
        _guard: &WalletGuard<'_>,
    ) -> Result<()> {
        let account = ctx.account().await?;
        let account_id_short = account.id().short();
        
        // Generate filename: use provided name or generate default
        let filename = if argv.is_empty() {
            // Generate default filename: transactions_<account_id>_<timestamp>.csv
            let timestamp = unixtime_as_millis_u64();
            format!("transactions_{}_{}.csv", account_id_short, timestamp)
        } else {
            let name = argv.remove(0);
            // Ensure .csv extension
            if name.ends_with(".csv") {
                name
            } else {
                format!("{}.csv", name)
            }
        };
        
        tprintln!(ctx, "");
        tprintln!(ctx, "Exporting transaction history to '{}'...", filename);
        
        // Export transactions using the account API
        let (csv_content, count) = account.export_transaction_history_csv(None, None, None).await?;
        
        if count == 0 {
            tprintln!(ctx, "No transactions found for this account.");
            return Ok(());
        }
        
        // Write to file
        match std::fs::write(&filename, &csv_content) {
            Ok(_) => {
                tprintln!(ctx, "Successfully exported {} transactions to '{}'", count.separated_string(), filename);
            }
            Err(e) => {
                terrorln!(ctx, "Failed to write CSV file '{}': {}", filename, e);
                return Err(Error::custom(format!("Failed to write CSV file: {}", e)));
            }
        }
        
        tprintln!(ctx, "");
        
        Ok(())
    }
}
