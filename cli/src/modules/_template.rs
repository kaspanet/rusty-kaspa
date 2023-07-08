use crate::imports::*;

#[derive(Default)]
pub struct Template;

#[async_trait]
impl Handler for Template {

    fn verb(&self, _ctx: &Arc<dyn Context>) -> Option<&'static str> {
        Some("template")
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        ""
    }

    async fn handle(self : Arc<Self>, ctx: &Arc<dyn Context>, argv : Vec<String>, cmd: &str) -> cli::Result<()> {
        self.main(ctx,argv,cmd).await.map_err(|e|e.into())
    }
}
