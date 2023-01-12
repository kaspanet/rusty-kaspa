// This attribute is required by BorshSerialize/Deserialize
#![recursion_limit = "256"]

pub mod api;
pub mod convert;
pub mod errors;
pub mod model;
pub mod notify;
pub mod stubs;
cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        pub mod server;
    }
}

pub mod prelude {
    pub use super::api::notifications::*;
    pub use super::model::address::*;
    pub use super::model::block::*;
    pub use super::model::blue_work::*;
    pub use super::model::hash::*;
    pub use super::model::header::*;
    pub use super::model::hex_cnv::*;
    pub use super::model::message::*;
    pub use super::model::script_class::*;
    pub use super::model::subnets::*;
    pub use super::model::tx::*;
}

pub use api::notifications::*;
pub use convert::*;
pub use errors::*;
pub use model::address::*;
pub use model::block::*;
pub use model::blue_work::*;
pub use model::hash::*;
pub use model::header::*;
pub use model::hex_cnv::*;
pub use model::message::*;
pub use model::script_class::*;
pub use model::subnets::*;
pub use model::tx::*;
