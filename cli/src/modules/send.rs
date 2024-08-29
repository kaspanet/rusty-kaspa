use crate::imports::*;

#[derive(Default, Handler)]
#[help("Send a Kaspa transaction to a public address")]
pub struct Send;

impl Send {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        // address, amount, priority fee
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let account = ctx.wallet().account()?;

        if argv.len() < 2 {
            tprintln!(ctx, "usage: send <address> <amount> <priority fee>");
            return Ok(());
        }

        let address = Address::try_from(argv.first().unwrap().as_str())?;
        let amount_sompi = try_parse_required_nonzero_kaspa_as_sompi_u64(argv.get(1))?;
        let priority_fee_sompi = try_parse_optional_kaspa_as_sompi_i64(argv.get(2))?.unwrap_or(0);
        let outputs = PaymentOutputs::from((address.clone(), amount_sompi));
        let abortable = Abortable::default();
        let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(Some(&account)).await?;

        // let ctx_ = ctx.clone();
        let (summary, _ids) = account
            .send(
                outputs.into(),
                priority_fee_sompi.into(),
                None,
                wallet_secret,
                payment_secret,
                &abortable,
                Some(Arc::new(move |_ptx| {
                    // tprintln!(ctx_, "Sending transaction: {}", ptx.id());
                })),
            )
            .await?;

        tprintln!(ctx, "Send - {summary}");
        tprintln!(ctx, "\nSending {} KAS to {address}, tx ids:", sompi_to_kaspa_string(amount_sompi));
        // tprintln!(ctx, "{}\n", ids.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));

        Ok(())
    }
}
