use convert_case::{Case, Casing};
use pad::PadStr;
use std::sync::Arc;
use workflow_core::enums::Describe;
use workflow_terminal::Terminal;

/// Actions - list of supported user commands.
/// If description starts with `!` it will be hidden from help output.
/// If description starts with `?` it will be greyed-out.

#[derive(Describe)]
pub enum Action {
    #[describe("Display this help")]
    Help,
    #[describe("Settings")]
    Set,
    #[describe("Select network type (mainnet|testnet)")]
    Network,
    #[describe("Connect to kaspa network")]
    Connect,
    #[describe("Disconnect from kaspa network")]
    Disconnect,
    #[describe("Import a wallet or a private key")]
    Import,
    #[describe("Create a new account or a wallet")]
    Create,
    #[describe("Open a wallet")]
    Open,
    #[describe("Close a wallet")]
    Close,
    #[describe("List wallet accounts")]
    List,
    #[describe("Select an account")]
    Select,
    #[describe("?Ping server (testing)")]
    Ping,
    #[describe("Get Info (testing)")]
    GetInfo,
    // #[describe("?Shows the balance of a public address")]
    // Balance,
    #[describe("?Broadcast the given transaction")]
    Broadcast,
    #[describe("?Create an unsigned Kaspa transaction")]
    CreateUnsignedTx,
    #[describe("?Prints the unencrypted wallet data")]
    DumpUnencrypted,
    #[describe("Generates new public address of the current wallet and shows it")]
    NewAddress,
    #[describe("?Parse the given transaction and print its contents")]
    // Parse,
    // #[describe("?Sends a Kaspa transaction to a public address")]
    Send,
    #[describe("?Shows address of the current wallet")]
    Address,
    #[describe("?Shows all generated public addresses of the current wallet")]
    ShowAddresses,
    #[describe("?Sign the given partially signed transaction")]
    Sign,
    #[describe("?Start the wallet daemon")]
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
    #[describe("Halt execution (testing)")]
    Halt,

    #[cfg(target_arch = "wasm32")]
    #[describe("!reload web interface (used for testing)")]
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
    let mut commands: Vec<(String, &str)> = Action::list()
        .iter()
        .map(|action| (action.as_str().from_case(Case::UpperCamel).to_case(Case::Kebab), action.describe()))
        .collect();
    commands.sort_by(|a, b| a.1.cmp(b.1));
    let len = commands.iter().map(|(c, _)| c.len()).fold(0, |a, b| a.max(b));
    for (cmd, help) in commands.iter() {
        let cmd = cmd.pad_to_width(len + 2);
        if !help.starts_with('!') {
            let (cmd, help) = if let Some(help) = help.strip_prefix('?') {
                let cmd = format!("\x1b[0;38;5;250m{cmd}\x1b[0m");
                let help = format!("\x1b[0;38;5;250m{help}\x1b[0m");
                (cmd, help)
            } else {
                (cmd, help.to_string())
            };
            term.writeln(format!("{:>4}{}{}", "", cmd.pad_to_width(len + 2), help).as_str());
        }
    }
    term.writeln("");
}
