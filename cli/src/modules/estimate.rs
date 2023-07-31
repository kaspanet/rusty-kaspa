use kaspa_wallet_core::tx::PaymentDestination;

use crate::imports::*;

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

        let amount = argv.get(0).unwrap();
        let priority_fee = argv.get(1);

        let amount_sompi = helpers::kas_str_to_sompi(amount)?;
        let priority_fee_sompi = if let Some(fee) = priority_fee { Some(helpers::kas_str_to_sompi(fee)?) } else { None };

        let abortable = Abortable::default();

        // just need any address for an estimate
        let change_address = account.change_address().await?;
        let destination = PaymentDestination::PaymentOutputs(PaymentOutputs::try_from((change_address.clone(), amount_sompi))?);
        let estimate = account.estimate(destination, priority_fee_sompi, false, None, &abortable).await?;

        tprintln!(ctx, "\nEstimate: {estimate:#?}");
        // tprintln!(ctx, "\nSending {amount} KAS to {address}, tx ids:");
        // tprintln!(ctx, "{}\n", ids.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));

        Ok(())
    }
}
