use crate::imports::*;

#[derive(Default, Handler)]
#[help("Displays this help message")]
pub struct Help;

impl Help {
    async fn main(self: Arc<Self>, dyn_ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let term = dyn_ctx.term();
        term.writeln("\nCommands:\n".crlf());

        let ctx = dyn_ctx.clone().downcast_arc::<KaspaCli>()?;
        let handlers = ctx.handlers().collect();
        let mut handlers =
            handlers.into_iter().filter_map(|h| h.verb(dyn_ctx).map(|verb| (verb, get_handler_help(h, dyn_ctx)))).collect::<Vec<_>>();

        handlers.sort_by_key(|(verb, _)| verb.to_string());
        let len = handlers.iter().map(|(c, _)| c.len()).fold(0, |a, b| a.max(b)) + 2;
        for (verb, help) in handlers {
            term.writeln(format!("{:>4} {} {}", "", verb.pad_to_width(len), help));
        }
        term.writeln("");

        Ok(())
    }
}
