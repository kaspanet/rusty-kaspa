use crate::imports::*;

#[derive(Default, Handler)]
#[help("Create a new wallet or an account")]
pub struct Create;

impl Create {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<WalletCli>()?;
        let wallet = ctx.wallet();

        let is_open = wallet.is_open()?;

        let op = if argv.is_empty() { if is_open { "account" } else { "wallet" }.to_string() } else { argv.remove(0) };

        match op.as_str() {
            "wallet" => {
                let wallet_name = if argv.is_empty() {
                    None
                } else {
                    let name = argv.remove(0);
                    let name = name.trim().to_string();

                    Some(name)
                };

                let wallet_name = wallet_name.as_deref();
                ctx.create_wallet(wallet_name).await?;
            }
            "account" => {
                if !is_open {
                    return Err(Error::WalletIsNotOpen);
                }

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

                // TODO - switch to selection; temporarily use existing account
                let account = wallet.account()?;
                let prv_key_data_id = account.prv_key_data_id;

                let account_name = account_name.as_deref();
                ctx.create_account(prv_key_data_id, account_kind, account_name).await?;
            }
            _ => {
                tprintln!(ctx, "\nError:\n");
                tprintln!(ctx, "Usage:\ncreate <account|wallet>");
                return Ok(());
            }
        }

        Ok(())
    }
}
