use crate::imports::*;
use crate::wizards;

#[derive(Default, Handler)]
#[help("Account management operations")]
pub struct Account;

impl Account {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let wallet = ctx.wallet();

        if !wallet.is_open() {
            return Err(Error::WalletIsNotOpen);
        }

        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        match argv.remove(0).as_str() {
            "name" => {
                if argv.len() != 1 {
                    tprintln!(ctx, "usage: 'account name <name>' or 'account name remove'");
                    return Ok(());
                } else {
                    let (secret, _) = ctx.ask_wallet_secret(None).await?;
                    let _ = ctx.notifier().show(Notification::Processing).await;
                    let account = ctx.select_account().await?;
                    let name = argv.remove(0);
                    if name == "remove" {
                        account.rename(secret, None).await?;
                    } else {
                        account.rename(secret, Some(name.as_str())).await?;
                    }
                }
            }
            "create" => {
                let account_kind = if argv.is_empty() {
                    AccountKind::Bip32
                } else {
                    let kind = argv.remove(0);
                    kind.parse::<AccountKind>()?
                };

                let account_name = if argv.is_empty() {
                    None
                } else {
                    let name = argv.remove(0);
                    let name = name.trim().to_string();

                    Some(name)
                };

                let prv_key_data_info = ctx.select_private_key().await?;

                let account_name = account_name.as_deref();
                wizards::account::create(&ctx, prv_key_data_info, account_kind, account_name).await?;
            }
            "scan" => {
                let extent = if argv.is_empty() {
                    100_000
                } else {
                    let extent = argv.remove(0);
                    extent.parse::<usize>()?
                };
                let window = if argv.is_empty() {
                    128
                } else {
                    let extent = argv.remove(0);
                    extent.parse::<usize>()?
                };
                self.derivation_scan(&ctx, extent, window, false).await?;
            }
            "sweep" => {
                let extent = if argv.is_empty() {
                    100_000
                } else {
                    let extent = argv.remove(0);
                    extent.parse::<usize>()?
                };
                let window = if argv.is_empty() {
                    128
                } else {
                    let extent = argv.remove(0);
                    extent.parse::<usize>()?
                };
                self.derivation_scan(&ctx, extent, window, true).await?;
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");
                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("create [<type>] [<name>]", "Create a new account (types: 'bip32' (default), 'legacy', 'multisig')"),
                // ("import", "Import a private key using 24 or 12 word mnemonic"),
                ("name <name>", "Name or rename the selected account (use 'remove' to remove the name"),
                ("scan [<derivations>]", "Scan extended address derivation chain (legacy accounts)"),
                ("sweep [<derivations>]", "Sweep extended address derivation chain (legacy accounts)"),
                // ("purge", "Purge an account from the wallet"),
            ],
            None,
        )?;

        Ok(())
    }

    async fn derivation_scan(self: &Arc<Self>, ctx: &Arc<KaspaCli>, extent: usize, window: usize, sweep: bool) -> Result<()> {
        let account = ctx.account().await?;
        let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(Some(&account)).await?;
        let _ = ctx.notifier().show(Notification::Processing).await;
        let abortable = Abortable::new();
        let ctx_ = ctx.clone();

        let account = account.as_derivation_capable()?;

        account
            .derivation_scan(
                wallet_secret,
                payment_secret,
                extent,
                window,
                sweep,
                &abortable,
                Some(Arc::new(move |processed: usize, balance, txid| {
                    if let Some(txid) = txid {
                        tprintln!(
                            ctx_,
                            "Scan detected {} KAS at index {}; transfer txid: {}",
                            sompi_to_kaspa_string(balance),
                            processed,
                            txid
                        );
                    } else {
                        tprintln!(ctx_, "Scanned {} derivations, found {} KAS", processed, sompi_to_kaspa_string(balance));
                    }
                })),
            )
            .await?;

        Ok(())
    }
}
