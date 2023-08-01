use crate::imports::*;
use crate::wizards;

#[derive(Default, Handler)]
#[help("Wallet management operations")]
pub struct Wallet;

impl Wallet {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        match argv.remove(0).as_str() {
            "create" => {
                let wallet_name = if argv.is_empty() {
                    None
                } else {
                    let name = argv.remove(0);
                    let name = name.trim().to_string();

                    Some(name)
                };

                let wallet_name = wallet_name.as_deref();
                wizards::wallet::create(&ctx, wallet_name).await?;
            }
            "open" => {
                let name = if let Some(name) = argv.first().cloned() {
                    Some(name)
                } else {
                    ctx.wallet().settings().get(WalletSettings::Wallet).clone()
                };

                let secret = Secret::new(ctx.term().ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
                let _ = ctx.notifier().show(Notification::Processing).await;
                ctx.wallet().load(secret, name).await?;
            }
            "close" => {
                ctx.wallet().close().await?;
            }
            "hint" => {
                if !argv.is_empty() {
                    let re = regex::Regex::new(r#"wallet\s+hint\s+"#).unwrap();
                    let hint = re.replace(cmd, "");
                    let hint = hint.trim();
                    let store = ctx.store();
                    if hint == "remove" {
                        tprintln!(ctx, "Hint is empty - removing wallet hint");
                        store.set_user_hint(None).await?;
                    } else {
                        store.set_user_hint(Some(hint.into())).await?;
                    }
                } else {
                    tprintln!(ctx, "Usage:\n'wallet hint <text>' or 'wallet hint remove' to remove the hint");
                }
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
                ("create [<name>]", "Create a new wallet"),
                ("open [<name>]", "Open an existing wallet (shorthand: 'open [<name>]')"),
                ("close", "Close an opened wallet (shorthand: 'close')"),
                ("hint", "Change the wallet phishing hint"),
            ],
            None,
        )?;

        Ok(())
    }
}
