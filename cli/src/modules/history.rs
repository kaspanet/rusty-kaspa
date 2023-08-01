use crate::imports::*;
use kaspa_wallet_core::error::Error as WalletError;
use kaspa_wallet_core::storage::Binding;
#[derive(Default, Handler)]
#[help("Display transaction history")]
pub struct History;

impl History {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        let last = if argv.is_empty() { None } else { argv[0].parse::<usize>().ok() };

        let account = ctx.account().await?;
        let network_id = ctx.wallet().network_id()?;
        let binding = Binding::from(&account);
        let current_daa_score = ctx.wallet().current_daa_score();
        let store = ctx.wallet().store().as_transaction_record_store()?;
        let mut ids = match store.transaction_id_iter(&binding, &network_id).await {
            Ok(ids) => ids,
            Err(err) => {
                if matches!(err, WalletError::NoRecordsFound) {
                    tprintln!(ctx);
                    tprintln!(ctx, "No transactions found for this account.");
                    tprintln!(ctx);
                } else {
                    terrorln!(ctx, "{err}");
                }
                return Ok(());
            }
        };
        let length = ids.size_hint().0;
        let skip = if let Some(last) = last {
            if last > length {
                0
            } else {
                length - last
            }
        } else {
            0
        };
        let mut index = 0;
        let page = 25;

        tprintln!(ctx);

        while let Some(id) = ids.try_next().await? {
            if index >= skip {
                if index > 0 && index % page == 0 {
                    tprintln!(ctx);
                    let prompt = format!(
                        "Displaying transactions {} to {} of {} (press any key to continue, 'Q' to abort)",
                        index.separated_string(),
                        (index + page).separated_string(),
                        length.separated_string()
                    );
                    let query = ctx.term().kbhit(&prompt).await?;
                    tprintln!(ctx);
                    if query.to_lowercase() == "q" {
                        return Ok(());
                    }
                }

                match store.load_single(&binding, &network_id, &id).await {
                    Ok(tx) => {
                        let text = tx.format_with_args(&ctx.wallet(), None, current_daa_score, true, Some(account.clone())).await;
                        tprintln!(ctx, "{text}");
                    }
                    Err(err) => {
                        terrorln!(ctx, "{err}");
                    }
                }
            }
            index += 1;
        }

        tprintln!(ctx);
        tprintln!(ctx, "{} transactions", length.separated_string());
        tprintln!(ctx);

        Ok(())
    }
}
