use crate::imports::*;

#[derive(Default, Handler)]
#[help("Exit this application")]
pub struct Exit;

impl Exit {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        ctx.wallet().select(None).await?;
        // ctx.term().refresh_prompt();
        // tprintln!(ctx, "{}", style("System is going down ...").blue());
        ctx.shutdown().await?;
        tprintln!(ctx, "bye!");

        // workflow_core::task::sleep(Duration::from_secs(10)).await;

        Ok(())
    }
}
