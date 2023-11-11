use crate::imports::*;

#[derive(Default, Handler)]
#[help("Exit the application")]
pub struct Exit;

impl Exit {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> cli::Result<()> {
        let term = ctx.term();

        term.writeln("bye!");
        #[cfg(not(target_arch = "wasm32"))]
        term.exit().await;
        #[cfg(target_arch = "wasm32")]
        workflow_dom::utils::window().location().reload().ok();

        Ok(())
    }
}
