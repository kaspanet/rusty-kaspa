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
            return Err("Usage: send <address> <amount> <priority fee>".into());
        }

        let address = argv.get(0).unwrap();
        let amount = argv.get(1).unwrap();
        let priority_fee = argv.get(2);

        let priority_fee_sompi = if let Some(fee) = priority_fee { Some(helpers::kas_str_to_sompi(fee)?) } else { None };

        let address = Address::try_from(address.as_str())?;
        let amount_sompi = helpers::kas_str_to_sompi(amount)?;

        let wallet_secret = Secret::new(ctx.term().ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
        // let mut payment_secret = Option::<Secret>::None;

        let payment_secret = if ctx.wallet().is_account_key_encrypted(&account).await?.is_some_and(|f| f) {
            Some(Secret::new(ctx.term().ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()))
        } else {
            None
        };
        // let keydata = self.wallet.get_prv_key_data(wallet_secret.clone(), &account.prv_key_data_id).await?;
        // if keydata.is_none() {
        //     return Err("It is read only wallet.".into());
        // }
        let abortable = Abortable::default();

        let outputs = PaymentOutputs::try_from((address.clone(), amount_sompi))?;

        // account.send(&address, amount_sompi, priority_fee_sompi, keydata.unwrap(), payment_secret, &abortable).await?;
        let ctx_ = ctx.clone();
        let ids = account
            .send(
                outputs.into(),
                priority_fee_sompi,
                false,
                None,
                wallet_secret,
                payment_secret,
                &abortable,
                Some(Arc::new(move |ptx| {
                    tprintln!(ctx_, "Sending transaction: {}", ptx.id());
                })),
            )
            .await?;

        tprintln!(ctx, "\nSending {amount} KAS to {address}, tx ids:");
        tprintln!(ctx, "{}\n", ids.into_iter().map(|a| a.to_string()).collect::<Vec<_>>().join("\n"));

        Ok(())
    }
}
