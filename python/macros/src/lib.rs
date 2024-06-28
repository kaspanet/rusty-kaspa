use proc_macro::TokenStream;
use proc_macro_error::proc_macro_error;

mod py_async;

#[proc_macro]
#[proc_macro_error]
pub fn py_async(input: TokenStream) -> TokenStream {
    py_async::py_async(input)
}
