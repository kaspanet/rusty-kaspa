use crate::helpers;
use crate::imports::*;

#[derive(Default, Handler)]
#[help("Track specific notifications when muted (balance|pending|tx|utxo|daa)")]
pub struct Track;

impl Track {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if let Some(attr) = argv.first() {
            let track: helpers::Track = attr.parse()?;
            ctx.flags().toggle(track);
        } else {
            for flag in ctx.flags().map().iter() {
                let k = flag.key().to_string();
                let v = flag.value().load(Ordering::SeqCst);
                let s = if v { "on" } else { "off" };
                tprintln!(ctx, "{k} is {s}");
            }
        }

        Ok(())
    }
}
