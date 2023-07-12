use crate::imports::*;

#[derive(Default, Handler)]
#[help("Exit this application")]
pub struct Exit;

impl Exit {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        tprintln!(ctx, "bye!");

        nw_sys::app::quit();

        Ok(())
    }
}
