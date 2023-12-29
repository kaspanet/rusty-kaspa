#[allow(clippy::module_inception)]
mod metrics;

mod ipc;
pub use ipc::*;
mod settings;
pub use settings::*;
mod toolbar;
