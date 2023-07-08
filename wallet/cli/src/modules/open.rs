use crate::imports::*;

#[derive(Default, Handler)]
#[help("Open a wallet")]
pub struct Open;

impl Open {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<WalletCli>()?;

        let name =
            if let Some(name) = argv.first().cloned() { Some(name) } else { ctx.wallet().settings().get(Settings::Wallet).clone() };

        let secret = Secret::new(ctx.term().ask(true, "Enter wallet password:").await?.trim().as_bytes().to_vec());
        ctx.wallet().load(secret, name).await?;

        Ok(())
    }
}
