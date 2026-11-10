extern crate self as aero_cli;

mod cli;
pub mod error;
pub mod extensions;
mod helpers;
mod imports;
mod matchers;
pub mod modules;
mod notifier;
pub mod result;
pub mod utils;
mod wizards;

pub use cli::{AeroCli, Options, TerminalOptions, TerminalTarget, aero_cli};
pub use workflow_terminal::Terminal;
