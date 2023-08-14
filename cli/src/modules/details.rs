use crate::imports::*;

#[derive(Default, Handler)]
#[help("Displays the detailed information about the currently selected account.")]
pub struct Details;

impl Details {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let account = ctx.select_account().await?.as_derivation_capable()?;

        let derivation = account.derivation();

        let manager = derivation.receive_address_manager();
        let index = manager.index()? + 1;
        let addresses = manager.get_range_with_args(0..index, false).await?;
        tprintln!(ctx, "Receive addresses: {index}");
        addresses.iter().for_each(|address| {
            tprintln!(ctx.term(), "{:>4}{}", "", style(address.to_string()).blue());
        });

        let manager = derivation.change_address_manager();
        let index = manager.index()? + 1;
        let addresses = manager.get_range_with_args(0..index, false).await?;
        tprintln!(ctx, "Change addresses: {index}");
        addresses.iter().for_each(|address| {
            tprintln!(ctx.term(), "{:>4}{}", "", style(address.to_string()).blue());
        });

        Ok(())
    }
}
