use proc_macro::TokenStream;
use proc_macro_error::proc_macro_error;
mod grpc;
mod handler;
mod wrpc;

#[proc_macro]
#[proc_macro_error]
pub fn build_wrpc_client_interface(input: TokenStream) -> TokenStream {
    wrpc::client::build_wrpc_client_interface(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn declare_typescript_wasm_interface(input: TokenStream) -> TokenStream {
    wrpc::wasm::declare_typescript_wasm_interface(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn build_wrpc_server_interface(input: TokenStream) -> TokenStream {
    wrpc::server::build_wrpc_server_interface(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn build_wrpc_wasm_bindgen_interface(input: TokenStream) -> TokenStream {
    wrpc::wasm::build_wrpc_wasm_bindgen_interface(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn build_wrpc_wasm_bindgen_subscriptions(input: TokenStream) -> TokenStream {
    wrpc::wasm::build_wrpc_wasm_bindgen_subscriptions(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn build_grpc_server_interface(input: TokenStream) -> TokenStream {
    grpc::server::build_grpc_server_interface(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn test_wrpc_serializer(input: TokenStream) -> TokenStream {
    wrpc::test::build_test(input)
}
