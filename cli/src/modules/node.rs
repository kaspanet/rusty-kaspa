use crate::imports::*;

#[derive(Default)]
pub struct Node;

#[async_trait]
impl Handler for Node {
    fn verb(&self, ctx: &Arc<dyn Context>) -> Option<&'static str> {
        if let Ok(ctx) = ctx.clone().downcast_arc::<KaspaCli>() {
            ctx.daemons().clone().kaspad.as_ref().map(|_| "node")
        } else {
            None
        }
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Manage local Kaspa node instance"
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Node {
    async fn main(self: Arc<Self>, _ctx: Arc<KaspaCli>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        tprintln!(_ctx, "TODO: node ctl..");

        Ok(())
    }
}
