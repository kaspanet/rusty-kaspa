use crate::imports::*;

#[derive(Default, Handler)]
#[help("Reload the web interface (used for testing)")]
pub struct Reload;

impl Reload {
    async fn main(self: Arc<Self>, _ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        #[cfg(target_arch = "wasm32")]
        workflow_dom::utils::window().location().reload().ok();
        Ok(())
    }
}
