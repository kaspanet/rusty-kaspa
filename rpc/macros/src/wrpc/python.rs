use crate::handler::*;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
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
                    fn #fn_call(&self, py: Python) -> PyResult<Py<PyAny>> {
                        // Returns result as JSON string
                        let client = self.client.clone();

                        // TODO - receive argument from Python and deserialize it
                        // explore https://docs.rs/serde-pyobject/latest/serde_pyobject/ for arg intake / return

                        // TODO replace serde_json with serde_pyobject
                        let request : #request_type = serde_json::from_str("{}").map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

                        let fut = async move {
                            let response : #response_type = client.#fn_call(request).await?;
                            // TODO - replace serde_json with serde_pyobject
                            serde_json::to_string(&response).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
                        };

                        let py_fut = pyo3_asyncio_0_21::tokio::future_into_py(py, fut)?;

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
