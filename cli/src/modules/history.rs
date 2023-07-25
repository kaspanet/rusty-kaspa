use crate::imports::*;
use kaspa_wallet_core::storage::Binding;

#[derive(Default, Handler)]
#[help("Display transaction history")]
pub struct History;

impl History {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let account = ctx.account().await?;
        let network_id = ctx.wallet().network()?;
        let binding = Binding::from(&account);
        let store = ctx.wallet().store().as_transaction_record_store()?;
        let mut ids = store.transaction_id_iter(&binding, &network_id).await?;
        while let Some(id) = ids.try_next().await? {
            let tx = store.load_single(&binding, &network_id, &id).await?;
            let text = tx.format(&ctx.wallet());
            tprintln!(ctx, "{text}");
            // .for_each(|line| tprintln!(ctx, "{line}"));
        }

        Ok(())
    }
}
