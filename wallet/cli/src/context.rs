// use std::sync::Arc;
// use kaspa_wallet_core::runtime;
// use workflow_terminal::cli::Context;
// use workflow_terminal::Terminal;

// pub struct Wallet {
//     wallet : Arc<runtime::Wallet>,
//     term : Arc<Terminal>,
// }

// impl Wallet {
//     pub fn new(term : &Arc<Terminal>, wallet : &Arc<runtime::Wallet>) -> Wallet {
//         Wallet {
//             wallet : wallet.clone(),
//             term : term.clone(),
//         }
//     }

//     pub fn wallet(&self) -> &Arc<runtime::Wallet> {
//         &self.wallet
//     }
// }

// impl Context for Wallet {
//     fn term(&self) -> Arc<Terminal> {
//         self.term.clone()
//     }
// }
