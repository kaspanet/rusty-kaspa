use crate::imports::*;
use kaspa_wallet_core::tx::PaymentDestination;
use kaspa_wallet_core::utils::*;

#[derive(Default, Handler)]
#[help("Estimate the fees for a transaction of a given amount")]
pub struct Estimate;

impl Estimate {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let account = ctx.wallet().account()?;

        if argv.is_empty() {
            return Err("Usage: estimate <amount> [<priority fee>]".into());
        }

        let amount = argv.get(0).cloned().unwrap_or_default();
        let amount_sompi = try_kaspa_str_to_sompi(amount)?.unwrap_or(0);
        let priority_fee_sompi = try_map_kaspa_str_to_sompi_i64(argv.get(1).map(String::as_str))?.unwrap_or(0);

        let abortable = Abortable::default();

        // just need any address for an estimate
        let change_address = account.change_address().await?;
        let destination = PaymentDestination::PaymentOutputs(PaymentOutputs::try_from((change_address.clone(), amount_sompi))?);
        let estimate = account.estimate(destination, priority_fee_sompi.into(), None, &abortable).await?;

        tprintln!(ctx, "\nEstimate: {estimate:#?}");

        Ok(())
    }
}
