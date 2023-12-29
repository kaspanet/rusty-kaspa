#[allow(clippy::module_inception)]
mod terminal;

mod ipc;
pub use ipc::*;
mod settings;
pub use settings::*;
