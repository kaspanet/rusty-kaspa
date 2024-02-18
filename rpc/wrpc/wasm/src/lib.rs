#![allow(unused_imports)]

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        mod imports;
        pub mod client;
        pub use client::*;
        pub mod beacon;
        pub use beacon::*;
        pub mod notify;
        pub use notify::*;
    }

}
