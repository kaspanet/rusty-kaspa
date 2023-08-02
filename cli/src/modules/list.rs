use crate::imports::*;

#[derive(Default, Handler)]
#[help("List wallet accounts and their balances")]
pub struct List;

impl List {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        ctx.list().await?;

        if !ctx.wallet().is_connected() {
            tprintln!(ctx, "{}", style("Wallet is not connected to the network").magenta());
            tprintln!(ctx);
        } else if !ctx.wallet().is_synced() {
            tprintln!(ctx, "{}", style("Kaspa node is currently syncing").magenta());
            tprintln!(ctx);
        }

        Ok(())
    }
}
