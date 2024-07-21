use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "py-sdk")] {
        pub mod client;
        pub mod resolver;
    }
}
