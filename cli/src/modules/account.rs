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
            "name" => {}
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

                let account = ctx.select_account().await?;
                let prv_key_data_id = account.prv_key_data_id;

                let account_name = account_name.as_deref();
                wizards::account::create(&ctx, prv_key_data_id, account_kind, account_name).await?;
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
                ("create [<type>]", "Create a new account (types: 'bip32' (default), 'legacy')"),
                // ("import", "Import a private key using 24 or 12 word mnemonic"),
                ("name <name>", "Name or rename account"),
                // ("purge", "Purge an account from the wallet"),
            ],
            None,
        )?;

        Ok(())
    }
}
