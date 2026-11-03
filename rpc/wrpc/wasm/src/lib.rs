//!
//! WASM bindings for the [Rusty Kaspa p2p Node wRPC Client](kaspa-wrpc-client)
//!
//! ## TLS crypto provider
//!
//! In WASM/JavaScript environments, TLS for `wss://` connections (and HTTPS via
//! the resolver) is handled by the host — the browser or Node.js — so, unlike
//! the native [`kaspa-wrpc-client`](https://docs.rs/kaspa-wrpc-client), **no
//! rustls crypto provider needs to be installed**.
//!

#![allow(unused_imports)]

use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        mod imports;
        pub mod client;
        pub use client::*;
        pub mod resolver;
        pub use resolver::*;
        pub mod notify;
        pub use notify::*;
    }

}
