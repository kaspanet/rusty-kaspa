use crate::imports::*;
use kaspa_wallet_core::tx::PaymentDestination;

#[derive(Default, Handler)]
#[help("Estimate the fees for a transaction of a given amount")]
pub struct Estimate;

impl Estimate {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let account = ctx.wallet().account()?;

        if argv.is_empty() {
            tprintln!(ctx, "usage: estimate <amount> [<priority fee>]");
            return Ok(());
        }

        let amount_sompi = try_parse_required_nonzero_kaspa_as_sompi_u64(argv.first())?;
        // TODO fee_rate
        let fee_rate = None;
        let priority_fee_sompi = try_parse_optional_kaspa_as_sompi_i64(argv.get(1))?.unwrap_or(0);
        let abortable = Abortable::default();

        // just use any address for an estimate (change address)
        let change_address = account.change_address()?;
        let destination = PaymentDestination::PaymentOutputs(PaymentOutputs::from((change_address.clone(), amount_sompi)));
        // TODO fee_rate
        let estimate = account.estimate(destination, fee_rate, priority_fee_sompi.into(), None, &abortable).await?;

        tprintln!(ctx, "Estimate - {estimate}");

        Ok(())
    }
}
