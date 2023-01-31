use convert_case::{Case, Casing};
use pad::PadStr;
use std::sync::Arc;
use workflow_core::enums::Describe;
use workflow_terminal::Terminal;

#[derive(Describe)]
pub enum Action {
    #[describe("Display this help")]
    Help,
    #[describe("Ping server (testing)")]
    Ping,
    #[describe("Get Info (testing)")]
    GetInfo,
    #[describe("Shows the balance of a public address")]
    Balance,
    #[describe("Broadcast the given transaction")]
    Broadcast,
    #[describe("Creates a new wallet")]
    Create,
    #[describe("Create an unsigned Kaspa transaction")]
    CreateUnsignedTx,
    #[describe("Prints the unencrypted wallet data")]
    DumpUnencrypted,
    #[describe("Generates new public address of the current wallet and shows it")]
    NewAddress,
    #[describe("Parse the given transaction and print its contents")]
    Parse,
    #[describe("Sends a Kaspa transaction to a public address")]
    Send,
    #[describe("Shows all generated public addresses of the current wallet")]
    ShowAddress,
    #[describe("Sign the given partially signed transaction")]
    Sign,
    #[describe("Start the wallet daemon")]
    // StartDaemon,
    // #[describe("Sends all funds associated with the given schnorr private key to a new address of the current wallet")]
    Sweep,

    // Notifications
    #[describe("Subscribe DAA score")]
    SubscribeDaaScore,
    #[describe("Unsubscribe DAA score")]
    UnsubscribeDaaScore,

    #[describe("Exit the wallet shell")]
    Exit,

    #[cfg(target_arch = "wasm32")]
    #[describe("hidden")]
    #[cfg(target_arch = "wasm32")]
    Reload,
}

impl TryInto<Action> for &str {
    type Error = String;
    fn try_into(self) -> std::result::Result<Action, Self::Error> {
        match Action::from_str(self.from_case(Case::Kebab).to_case(Case::UpperCamel).as_str()) {
            Some(action) => Ok(action),
            None => Err(format!("command not found: {self}")),
        }
    }
}

pub fn display_help(term: &Arc<Terminal>) {
    let commands: Vec<(String, &str)> = Action::list()
        .iter()
        .map(|action| (action.as_str().from_case(Case::UpperCamel).to_case(Case::Kebab), action.describe()))
        .collect();
    let len = commands.iter().map(|(c, _)| c.len()).fold(0, |a, b| a.max(b));

    for (cmd, help) in commands.iter() {
        if *help != "hidden" {
            term.writeln(format!("{:>4}{}{}", "", cmd.pad_to_width(len + 2), help).as_str());
        }
    }
    term.writeln("");
}
