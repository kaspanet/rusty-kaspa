use crate::handler::*;
use convert_case::{Case, Casing};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use regex::Regex;
use std::convert::Into;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Error, Expr, ExprArray, Result, Token,
};

#[derive(Debug)]
struct RpcTable {
    handlers: ExprArray,
}

impl Parse for RpcTable {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 1 {
            return Err(Error::new_spanned(parsed, "usage: build_wrpc_python_interface!([getInfo, ..])".to_string()));
        }

        let mut iter = parsed.iter();
        // Intake enum variants as an array
        let handlers = get_handlers(iter.next().unwrap().clone())?;

        Ok(RpcTable { handlers })
    }
}

impl ToTokens for RpcTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets = Vec::new();

        for handler in self.handlers.elems.iter() {
            let Handler { fn_call, request_type, response_type, .. } = Handler::new(handler);

            targets.push(quote! {

                #[pymethods]
                impl RpcClient {
                    fn #fn_call(&self, py: Python, request: Py<PyDict>) -> PyResult<Py<PyAny>> {
                        let client = self.inner.client.clone();

                        let request : #request_type = serde_pyobject::from_pyobject(request.into_bound(py)).unwrap();

                        let py_fut = pyo3_asyncio_0_21::tokio::future_into_py(py, async move {
                            let response : #response_type = client.#fn_call(request).await?;
                            Python::with_gil(|py| {
                                Ok(serde_pyobject::to_pyobject(py, &response).unwrap().to_object(py))
                            })
                        })?;

                        Python::with_gil(|py| Ok(py_fut.into_py(py)))
                    }
                }
            });
        }

        quote! {
            #(#targets)*
        }
        .to_tokens(tokens);
    }
}

pub fn build_wrpc_python_interface(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcTable);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}

#[derive(Debug)]
struct RpcSubscriptions {
    handlers: ExprArray,
}

impl Parse for RpcSubscriptions {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 1 {
            return Err(Error::new_spanned(parsed, "usage: build_wrpc_python_!([getInfo, ..])".to_string()));
        }

        let mut iter = parsed.iter();
        // Intake enum variants as an array
        let handlers = get_handlers(iter.next().unwrap().clone())?;

        Ok(RpcSubscriptions { handlers })
    }
}

impl ToTokens for RpcSubscriptions {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets = Vec::new();

        for handler in self.handlers.elems.iter() {
            // TODO docs (name, docs)
            let (name, _) = match handler {
                syn::Expr::Path(expr_path) => (expr_path.path.to_token_stream().to_string(), &expr_path.attrs),
                _ => {
                    continue;
                }
            };

            let name = format!("Notify{}", name.as_str());
            let regex = Regex::new(r"^Notify").unwrap();
            let blank = regex.replace(&name, "");
            let subscribe = regex.replace(&name, "Subscribe");
            let unsubscribe = regex.replace(&name, "Unsubscribe");
            let scope = Ident::new(&blank, Span::call_site());
            let sub_scope = Ident::new(format!("{blank}Scope").as_str(), Span::call_site());
            let fn_subscribe_snake = Ident::new(&subscribe.to_case(Case::Snake), Span::call_site());
            let fn_unsubscribe_snake = Ident::new(&unsubscribe.to_case(Case::Snake), Span::call_site());

            targets.push(quote! {
                #[pymethods]
                impl RpcClient {
                    fn #fn_subscribe_snake(&self, py: Python) -> PyResult<Py<PyAny>> {
                        if let Some(listener_id) = self.listener_id() {
                            let client = self.inner.client.clone();
                            py_async! {py, async move {
                                client.start_notify(listener_id, Scope::#scope(#sub_scope {})).await?;
                                Ok(())
                            }}
                        } else {
                            Err(PyErr::new::<PyException, _>("RPC subscribe on a closed connection"))
                        }
                    }

                    fn #fn_unsubscribe_snake(&self, py: Python) -> PyResult<Py<PyAny>> {
                        if let Some(listener_id) = self.listener_id() {
                            let client = self.inner.client.clone();
                            py_async! {py, async move {
                                client.stop_notify(listener_id, Scope::#scope(#sub_scope {})).await?;
                                Ok(())
                            }}
                        } else {
                            Err(PyErr::new::<PyException, _>("RPC unsubscribe on a closed connection"))
                        }
                    }
                }
            });
        }

        quote! {
            #(#targets)*
        }
        .to_tokens(tokens);
    }
}

pub fn build_wrpc_python_subscriptions(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcSubscriptions);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}
