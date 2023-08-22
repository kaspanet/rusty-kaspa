use crate::imports::*;

#[derive(Default, Handler)]
#[help("Produce an error message (testing)")]
pub struct Error;

impl Error {
    async fn main(self: Arc<Self>, _ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        log_error!("this is an error message");
        Ok(())
    }
}
