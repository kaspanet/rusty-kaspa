use crate::error::Error;
use crate::imports::Handler;
use crate::{wizards, KaspaCli};
use kaspa_wallet_core::storage::account::HtlcRole;
use std::ops::Deref;
use std::sync::Arc;
use workflow_terminal::{tprintln, Context};
#[derive(Default, Handler)]
#[help("HTLC management operations")]
pub struct Htlc;

impl Htlc {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> crate::imports::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let wallet = ctx.wallet();

        if !wallet.is_open() {
            return Err(Error::WalletIsNotOpen);
        }

        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        let action = argv.remove(0);

        match action.as_str() {
            "create" => {
                let mut argv = argv.into_iter();
                let role = match argv.next().as_deref() {
                    Some("sender") => HtlcRole::Sender,
                    Some("receiver") => HtlcRole::Receiver,
                    _ => return Err(Error::Custom("unknown htlc role".to_string())),
                };
                let locktime = argv
                    .next()
                    .map(|s| s.parse())
                    .ok_or(Error::Custom("missing locktime".to_owned()))?
                    .map_err(|_| Error::Custom("wrong type of locktime".to_owned()))?;
                let secret_hash = argv.next().ok_or(Error::Custom("missing secret hash".to_owned()))?;
                let account_name = argv.next();
                wizards::htlc::create(&ctx, account_name.as_deref(), role, locktime, secret_hash).await?;
            }
            "secret-add" => {
                let secret = argv.first().ok_or(Error::Custom("'secret' is required".to_string()))?;
                let account = ctx.select_account().await?;
                wizards::htlc::add_secret(&ctx, account, secret.clone()).await?;
                tprintln!(ctx, "secret successfully added\r\n");
            }
            "show" => {
                let account = ctx.select_account().await?;
                let address = wizards::htlc::show(&ctx, account).await?;
                tprintln!(ctx, "address of private key holder: '{address}'\r\n");
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");
                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> crate::imports::Result<()> {
        ctx.term().help(
            &[
                (
                    "create <role> <locktime> <secret-hash> [<name>]",
                    "Create a new account (role: 'sender', 'receiver', locktime: unix time in ms, secret-hash: hexed hash of secret)",
                ),
                ("secret-add <secret in hex>", "Provide secret to make it possible to withdraw"),
            ],
            None,
        )?;

        Ok(())
    }
}
