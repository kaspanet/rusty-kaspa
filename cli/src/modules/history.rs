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
        let current_daa_score = ctx.wallet().current_daa_score();
        let store = ctx.wallet().store().as_transaction_record_store()?;
        let mut ids = store.transaction_id_iter(&binding, &network_id).await?;
        while let Some(id) = ids.try_next().await? {
            match store.load_single(&binding, &network_id, &id).await {
                Ok(tx) => {
                    let text = tx.format_with_args(&ctx.wallet(), None, current_daa_score);
                    tprintln!(ctx, "{text}");
                }
                Err(err) => {
                    terrorln!(ctx, "{err}");
                }
            }
        }

        Ok(())
    }
}
