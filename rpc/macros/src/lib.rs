use proc_macro::TokenStream;
use proc_macro_error::proc_macro_error;
mod wrpc;

#[proc_macro]
#[proc_macro_error]
pub fn build_wrpc_client_interface(input: TokenStream) -> TokenStream {
    wrpc::client::build_wrpc_client_interface(input)
}

#[proc_macro]
#[proc_macro_error]
pub fn build_wrpc_server_interface(input: TokenStream) -> TokenStream {
    wrpc::server::build_wrpc_server_interface(input)
}
