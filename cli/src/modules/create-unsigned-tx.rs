use crate::imports::*;

#[derive(Default, Handler)]
pub struct CreateUnsignedTx;

impl CreateUnsignedTx {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let _account = ctx.wallet().account()?;
        // TODO account.create_unsigned_transaction().await?;
        Ok(())
    }
}
