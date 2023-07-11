use crate::imports::*;

#[derive(Default, Handler)]
#[help("Displays the detailed information about the currently selected account.")]
pub struct Details;

impl Details {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        tprintln!(ctx, "Change addresses: 0..");

        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let wallet = ctx.wallet();

        let manager = wallet.account()?.receive_address_manager()?;
        let index = manager.index()?;
        let addresses = manager.get_range_with_args(0..index, false).await?;
        tprintln!(ctx, "Receive addresses: 0..{index}");
        tprintln!(ctx, "{}\n", addresses.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));

        let manager = wallet.account()?.change_address_manager()?;
        let index = manager.index()?;
        let addresses = manager.get_range_with_args(0..index, false).await?;
        tprintln!(ctx, "Change addresses: 0..{index}");
        tprintln!(ctx.term(), "{}\n", addresses.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));

        Ok(())
    }
}
