use crate::imports::*;

#[derive(Default, Handler)]
#[help("Change the wallet or account name")]
pub struct Name;

impl Name {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<WalletCli>()?;

        if argv.is_empty() {
            let account = ctx.account().await?;
            let id = account.id().to_hex();
            let name = account.name();
            let name = if name.is_empty() { "no name".to_string() } else { name };

            tprintln!(ctx, "\nname: {name}  account id: {id}\n");
        } else {
            // let account = self.account().await?;
        }

        Ok(())
    }
}
