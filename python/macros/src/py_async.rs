use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use std::convert::Into;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Error, Expr, ExprAsync, Result, Token,
};

#[derive(Debug)]
struct PyAsync {
    py: Expr,
    block: ExprAsync,
}

impl Parse for PyAsync {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 2 {
            return Err(Error::new_spanned(parsed, "usage: py_async!{py, async move { Ok(()) }}".to_string()));
        }

        let mut iter = parsed.iter();
        // python object (py: Python)
        let py = iter.next().unwrap().clone();

        // async block to encapsulate
        let block = match iter.next().unwrap().clone() {
            Expr::Async(block) => block,
            statement => {
                return Err(Error::new_spanned(statement, "the argument must be an async block".to_string()));
            }
        };

        Ok(PyAsync { py, block })
    }
}

impl ToTokens for PyAsync {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let PyAsync { py, block } = self;

        quote! {
            let __fut__ = #block;
            let __py_fut__ = pyo3_async_runtimes::tokio::future_into_py(#py, __fut__)?;
            pyo3::prelude::Python::with_gil(|py| Ok(__py_fut__.into_py(#py)))
        }
        .to_tokens(tokens);
    }
}

pub fn py_async(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let py_async = parse_macro_input!(input as PyAsync);
    let token_stream = py_async.to_token_stream();
    // println!("MACRO: {}", token_stream.to_string());
    token_stream.into()
}
