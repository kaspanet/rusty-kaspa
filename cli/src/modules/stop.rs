use crate::imports::*;

#[derive(Default, Handler)]
#[help("Stop local node and close wallet")]
pub struct Stop;

impl Stop {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        ctx.term().exec("wallet close").await?;
        ctx.term().exec("disconnect").await?;
        ctx.term().exec("node stop").await?;

        Ok(())
    }
}
