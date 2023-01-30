// This attribute is required by BorshSerialize/Deserialize
#![recursion_limit = "256"]

pub mod api;
pub mod convert;
pub mod error;
pub mod model;
pub mod notify;
cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        pub mod server;
    }
}

pub mod prelude {
    pub use super::api::notifications::*;
    pub use super::model::script_class::*;
    pub use super::model::*;
    pub use super::notify::listener::*;
    pub use super::notify::message::*;
    pub use super::notify::notifier::*;
}

pub use api::notifications::*;
pub use convert::*;
pub use error::*;
pub use model::script_class::*;
pub use model::*;
pub use notify::message::*;
