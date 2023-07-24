use crate::imports::*;

#[derive(Default, Handler)]
#[help("Basic command guide for using this software.")]
pub struct Guide;

impl Guide {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> cli::Result<()> {
        let term = ctx.term();
        let guide = include_str!("guide.txt");

        term.writeln(guide.crlf());

        Ok(())
    }
}
