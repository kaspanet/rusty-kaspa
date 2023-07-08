use crate::imports::*;

#[derive(Default, Handler)]
#[help("Estimate the fees for a transaction of a given amount")]
pub struct Estimate;

impl Estimate {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<WalletCli>()?;
        ctx.term().writeln("estimation is not implemented");
        Ok(())
    }
}
