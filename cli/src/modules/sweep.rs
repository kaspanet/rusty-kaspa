use crate::imports::*;

#[derive(Default, Handler)]
#[help("Sends all funds associated with the given account to a new address of theis account")]
pub struct Sweep;

impl Sweep {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        ctx.wallet().account()?.sweep().await?;
        Ok(())
    }
}
