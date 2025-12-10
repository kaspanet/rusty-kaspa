use crate::imports::*;

#[derive(Default, Handler)]
#[help("Select an account")]
pub struct Select;

impl Select {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        #[cfg(not(feature = "multi-user"))]
        {
            let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
            let mut argv = argv;
            if argv.is_empty() {
                let account = ctx.select_account().await?;
                ctx.wallet().select(Some(&account)).await?;
            } else {
                let pat = argv.remove(0);
                let pat = pat.trim();

                let account = ctx.find_accounts_by_name_or_id(pat).await?;
                ctx.wallet().select(Some(&account)).await?;
            }
            Ok(())
        }
        #[cfg(feature = "multi-user")]
        {
            let _ = (ctx, argv);
            Err("Account selection is not available in multi-user mode".into())
        }
    }
}
