use crate::imports::*;

#[derive(Default, Handler)]
#[help("Mute (toggle notification output mute)")]
pub struct Mute;

impl Mute {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        tprintln!(ctx, "mute is {}", ctx.toggle_mute());
        Ok(())
    }
}
