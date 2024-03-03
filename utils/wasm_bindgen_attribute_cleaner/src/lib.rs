extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemStruct, visit_mut::VisitMut, Field};

struct AttrCleaner;

impl VisitMut for AttrCleaner {
    fn visit_field_mut(&mut self, field: &mut Field) {
        // Retain only those attributes that do NOT match the `wasm_bindgen` path
        field.attrs.retain(|attr| !attr.path.is_ident("wasm_bindgen"));
    }
}

#[proc_macro_attribute]
pub fn clean_attributes(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(item as ItemStruct);
    syn::visit_mut::visit_item_struct_mut(&mut AttrCleaner, &mut ast);

    TokenStream::from(quote! { #ast })
}