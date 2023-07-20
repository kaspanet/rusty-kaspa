use crate::imports::*;

#[derive(Default, Handler)]
#[help("Generate a new address for the current account")]
pub struct NewAddress;

impl NewAddress {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let account = ctx.wallet().account()?;
        let response = account.new_receive_address().await?;
        tprintln!(ctx, "{response}");
        Ok(())
    }
}
