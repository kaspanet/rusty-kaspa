use crate::imports::*;

#[derive(Default, Handler)]
#[help("Open a wallet (shorthand for 'wallet open [<name>]')")]
pub struct Open;

impl Open {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, cmd: &str) -> Result<()> {
        Ok(ctx.term().exec(format!("wallet {cmd}")).await?)
    }
}
