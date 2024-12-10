use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, Path};

pub fn namespace(attr: TokenStream, item: TokenStream) -> TokenStream {
    let api_namespace = parse_macro_input!(attr as Path);
    let mut func = parse_macro_input!(item as ItemFn);

    let check = syn::parse2(quote! {
        if !self.namespaces.is_enabled(&#api_namespace) {
            // As macro processing happens after async_trait processing its wrapped with async_trait return type
            return std::boxed::Box::pin(std::future::ready(Err(RpcError::UnauthorizedMethod(#api_namespace.to_string()))));
        }
    })
    .unwrap();

    func.block.stmts.insert(0, check);
    quote!(#func).into()
}
