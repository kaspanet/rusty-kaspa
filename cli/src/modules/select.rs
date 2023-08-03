use crate::imports::*;

#[derive(Default, Handler)]
#[help("Select an account")]
pub struct Select;

impl Select {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if argv.is_empty() {
            let account = ctx.select_account().await?;
            ctx.wallet().select(Some(&account)).await?;
        } else {
            let pat = argv.remove(0);
            let pat = pat.trim();

            let account = ctx.find_accounts_by_name_or_id(pat).await?;
            ctx.wallet().select(Some(&account)).await?;

            // let matches = ctx.wallet().find_accounts_by_name_or_id(pat).await?;
            // if matches.is_empty() {
            //     return Err(Error::AccountNotFound(pat.to_string()));
            // } else if matches.len() > 1 {
            //     return Err(Error::AmbigiousAccount(pat.to_string()));
            // } else {
            //     let account = matches[0].clone();
            //     ctx.wallet().select(Some(&account)).await?;
            // };
        }

        Ok(())
    }
}
