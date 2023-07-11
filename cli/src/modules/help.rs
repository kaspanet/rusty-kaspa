use crate::imports::*;

#[derive(Default, Handler)]
#[help("Displays this message")]
pub struct Help;
// declare_handler!(Help,"Display this help message");

impl Help {
    async fn main(self: Arc<Self>, dyn_ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        let term = dyn_ctx.term();
        term.writeln("\nCommands:\n".crlf());

        let ctx = dyn_ctx.clone().downcast_arc::<KaspaCli>()?;
        let handlers = ctx.handler().collect();
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

// pub fn display_help(term: &Arc<Terminal>) {
//     let mut commands: Vec<(String, &str)> = Action::list()
//         .iter()
//         .map(|action| (action.as_str().from_case(Case::UpperCamel).to_case(Case::Kebab), action.describe()))
//         .collect();
//     commands.sort_by(|a, b| a.1.cmp(b.1));
//     let len = commands.iter().map(|(c, _)| c.len()).fold(0, |a, b| a.max(b));
//     for (cmd, help) in commands.iter() {
//         let cmd = cmd.pad_to_width(len + 2);
//         if !help.starts_with('!') {
//             let (cmd, help) = if let Some(help) = help.strip_prefix('?') {
//                 let cmd = format!("\x1b[0;38;5;250m{cmd}\x1b[0m");
//                 let help = format!("\x1b[0;38;5;250m{help}\x1b[0m");
//                 (cmd, help)
//             } else {
//                 (cmd, help.to_string())
//             };
//             term.writeln(format!("{:>4}{}{}", "", cmd.pad_to_width(len + 2), help).as_str());
//         }
//     }
//     term.writeln("");
// }
