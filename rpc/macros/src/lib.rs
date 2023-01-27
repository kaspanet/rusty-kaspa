use proc_macro::TokenStream;
use proc_macro_error::proc_macro_error;
mod wrpc;

#[proc_macro]
#[proc_macro_error]
pub fn build_wrpc_interface(input: TokenStream) -> TokenStream {
    wrpc::build_wrpc_interface(input)
}
