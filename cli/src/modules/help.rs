use crate::imports::*;

#[derive(Default, Handler)]
#[help("Displays this help message")]
pub struct Help;

impl Help {
    async fn main(self: Arc<Self>, dyn_ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let term = dyn_ctx.term();
        term.writeln("\nCommands:".crlf());

        let ctx = dyn_ctx.clone().downcast_arc::<KaspaCli>()?;
        let handlers = ctx.handlers().collect();
        let handlers =
            handlers.into_iter().filter_map(|h| h.verb(dyn_ctx).map(|verb| (verb, get_handler_help(h, dyn_ctx)))).collect::<Vec<_>>();

        term.help(&handlers, None)?;

        Ok(())
    }
}
