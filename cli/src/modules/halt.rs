use crate::imports::*;

#[derive(Default, Handler)]
#[help("Halt execution (used for testing)")]
pub struct Halt;

impl Halt {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        tprintln!(ctx, "halt");
        panic!("halting on user request...");
    }
}
