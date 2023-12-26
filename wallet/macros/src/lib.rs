use proc_macro::TokenStream;
use proc_macro_error::proc_macro_error;
mod handler;
mod wallet;

#[proc_macro]
#[proc_macro_error]
pub fn build_wallet_client_transport_interface(input: TokenStream) -> TokenStream {
    wallet::client::build_transport_interface(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn build_wallet_server_transport_interface(input: TokenStream) -> TokenStream {
    wallet::server::build_transport_interface(input)
}

// #[proc_macro]
// #[proc_macro_error]
// pub fn build_wrpc_wasm_bindgen_interface(input: TokenStream) -> TokenStream {
//     wallet::wasm::build_wrpc_wasm_bindgen_interface(input)
// }

// #[proc_macro]
// #[proc_macro_error]
// pub fn build_wrpc_wasm_bindgen_subscriptions(input: TokenStream) -> TokenStream {
//     wallet::wasm::build_wrpc_wasm_bindgen_subscriptions(input)
// }
