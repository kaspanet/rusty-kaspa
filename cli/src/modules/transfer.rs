use crate::imports::*;
use kaspa_wallet_core::utils::*;

#[derive(Default, Handler)]
#[help("Transfer funds between wallet accounts")]
pub struct Transfer;

impl Transfer {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let account = ctx.wallet().account()?;

        if argv.len() < 2 {
            return Err("Usage: transfer <account> <amount> <priority fee>".into());
        }

        let target_account = argv.get(0).unwrap();
        let amount = argv.get(1).unwrap();
        let amount_sompi = helpers::kas_str_to_sompi(amount)?.ok_or_else(|| Error::custom("Invalid amount"))?;
        let priority_fee = argv.get(2).map(String::as_str);
        let priority_fee_sompi = try_map_kaspa_str_to_sompi_i64(priority_fee)?.unwrap_or(0);

        let target_account = ctx.find_accounts_by_name_or_id(target_account).await?;
        if target_account.id == account.id {
            return Err("Cannot transfer to the same account".into());
        }
        let target_address = target_account.receive_address().await?;
        let wallet_secret = Secret::new(ctx.term().ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());

        let payment_secret = if ctx.wallet().is_account_key_encrypted(&account).await?.is_some_and(|f| f) {
            Some(Secret::new(ctx.term().ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec()))
        } else {
            None
        };

        let abortable = Abortable::default();
        let outputs = PaymentOutputs::try_from((target_address.clone(), amount_sompi))?;

        let ctx_ = ctx.clone();
        let (summary, _ids) = account
            .send(
                outputs.into(),
                priority_fee_sompi.into(),
                None,
                wallet_secret,
                payment_secret,
                &abortable,
                Some(Arc::new(move |ptx| {
                    tprintln!(ctx_, "Sending transaction: {}", ptx.id());
                })),
            )
            .await?;

        tprintln!(ctx, "Transfer: {summary}");

        Ok(())
    }
}
