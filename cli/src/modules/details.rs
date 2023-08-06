use crate::imports::*;

#[derive(Default, Handler)]
#[help("Displays the detailed information about the currently selected account.")]
pub struct Details;

impl Details {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let account = ctx.select_account().await?;

        let manager = account.receive_address_manager()?;
        let index = manager.index()? + 1;
        let addresses = manager.get_range_with_args(0..index, false).await?;
        tprintln!(ctx, "Receive addresses: {index}");
        addresses.iter().for_each(|address| {
            tprintln!(ctx.term(), "{:>4}{}", "", style(address.to_string()).blue());
        });

        let manager = account.change_address_manager()?;
        let index = manager.index()? + 1;
        let addresses = manager.get_range_with_args(0..index, false).await?;
        tprintln!(ctx, "Change addresses: {index}");
        addresses.iter().for_each(|address| {
            tprintln!(ctx.term(), "{:>4}{}", "", style(address.to_string()).blue());
        });

        Ok(())
    }
}
