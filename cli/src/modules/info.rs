use crate::imports::*;

#[derive(Default, Handler)]
#[help("Get connected node information")]
pub struct Info;

impl Info {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let response = ctx.wallet().get_info().await?;
        tprintln!(ctx, "{response}");
        Ok(())
    }
}
