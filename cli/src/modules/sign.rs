use crate::imports::*;

#[derive(Default, Handler)]
#[help("Sign the given partially signed transaction")]
pub struct Sign;

impl Sign {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let _ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        // TODO - ctx.wallet().account()?.sign().await?;

        Ok(())
    }
}
