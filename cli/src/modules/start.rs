use crate::imports::*;

#[derive(Default, Handler)]
#[help("Stop local node and close wallet")]
pub struct Start;

impl Start {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        // - TODO - check states

        ctx.term().exec("wallet open").await?;
        ctx.term().exec("node start").await?;

        Ok(())
    }
}
