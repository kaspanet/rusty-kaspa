use crate::imports::*;

#[derive(Default, Handler)]
#[help("Change the wallet phishing hint")]
pub struct Hint;

impl Hint {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if !argv.is_empty() {
            let hint = cmd.replace("hint", "");
            let hint = hint.trim();
            let store = ctx.store();
            if hint == "remove" {
                store.set_user_hint(None).await?;
            } else {
                store.set_user_hint(Some(hint.into())).await?;
            }
        } else {
            tprintln!(ctx, "Usage:\n'hint <text>' or 'hint remove'");
        }

        Ok(())
    }
}
