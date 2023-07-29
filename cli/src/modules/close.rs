use crate::imports::*;

#[derive(Default, Handler)]
#[help("Close an opened wallet")]
pub struct Close;

impl Close {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        ctx.wallet().close().await?;
        Ok(())
    }
}
