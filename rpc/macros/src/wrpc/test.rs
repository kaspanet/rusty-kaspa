use convert_case::{Case, Casing};
use proc_macro2::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{quote, ToTokens};
use std::convert::Into;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Error, Expr, Result, Token,
};

#[derive(Debug)]
struct TestTable {
    rpc_op: Expr,
}

impl Parse for TestTable {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 1 {
            return Err(Error::new_spanned(parsed, "usage: test!(GetInfo)".to_string()));
        }

        let mut iter = parsed.iter();
        let rpc_op = iter.next().unwrap().clone();

        Ok(TestTable { rpc_op })
    }
}

impl ToTokens for TestTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let rpc_op = &self.rpc_op;

        let (name, _docs) = match rpc_op {
            syn::Expr::Path(expr_path) => (expr_path.path.to_token_stream().to_string(), expr_path.attrs.clone()),
            _ => (rpc_op.to_token_stream().to_string(), vec![]),
        };
        let typename = Ident::new(&name.to_string(), Span::call_site());
        let fn_test = Ident::new(&format!("test_wrpc_serializer_{}", name.to_case(Case::Snake)), Span::call_site());

        quote! {

            #[test]
            fn #fn_test() {
                test::<#typename>(#name);
            }

        }
        .to_tokens(tokens);
    }
}

pub fn build_test(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as TestTable);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}
