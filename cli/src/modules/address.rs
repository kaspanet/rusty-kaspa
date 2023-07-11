use crate::imports::*;

#[derive(Default, Handler)]
#[help("Show the address of the current wallet account")]
pub struct Address;

impl Address {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let address = ctx.account().await?.receive_address().await?.to_string();
        tprintln!(ctx, "\n{address}\n");
        Ok(())
    }
}
