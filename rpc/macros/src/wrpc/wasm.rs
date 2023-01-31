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
    handlers_no_args: ExprArray,
    handlers_with_args: ExprArray,
}

impl Parse for RpcTable {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 2 {
            return Err(Error::new_spanned(
                parsed,
                "usage: build_wrpc_wasm_bindgen_interface!([fn no args, ..],[fn with args, ..])".to_string(),
            ));
        }

        let mut iter = parsed.iter();
        let handlers_no_args = get_handlers(iter.next().unwrap().clone())?;
        let handlers_with_args = get_handlers(iter.next().unwrap().clone())?;

        let handlers = RpcTable { handlers_no_args, handlers_with_args };
        Ok(handlers)
    }
}

impl ToTokens for RpcTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets_no_args = Vec::new();
        let mut targets_with_args = Vec::new();

        for handler in self.handlers_no_args.elems.iter() {
            let Handler { fn_call, fn_no_suffix, request_type, response_type, .. } = Handler::new(handler);

            targets_no_args.push(quote! {

                pub async fn #fn_no_suffix(&self) -> JsResult<JsValue> {
                    let value: JsValue = js_sys::Object::new().into();
                    let request: #request_type = from_value(value)?;
                    log_info!("request: {:#?}",request);
                    let result: RpcResult<#response_type> = self.client.#fn_call(request).await;
                    log_info!("result: {:#?}",result);

                    let response: #response_type = result.map_err(|err|wasm_bindgen::JsError::new(&err.to_string()))?;
                    log_info!("response: {:#?}",response);
                    to_value(&response).map_err(|err|err.into())
                }

            });
        }

        for handler in self.handlers_with_args.elems.iter() {
            let Handler { fn_call, fn_no_suffix, request_type, response_type, .. } = Handler::new(handler);

            targets_with_args.push(quote! {

                pub async fn #fn_no_suffix(&self, request: JsValue) -> JsResult<JsValue> {
                    let request: #request_type = from_value(request)?;
                    let result: RpcResult<#response_type> = self.client.#fn_call(request).await;
                    let response: #response_type = result.map_err(|err|wasm_bindgen::JsError::new(&err.to_string()))?;
                    to_value(&response).map_err(|err|err.into())
                }

            });
        }

        quote! {
            #[wasm_bindgen]
            impl RpcClient {
                #(#targets_no_args)*
                #(#targets_with_args)*
            }
        }
        .to_tokens(tokens);
    }
}

pub fn build_wrpc_wasm_bindgen_interface(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcTable);
    let ts = rpc_table.to_token_stream();
    // println!("ts====>: {:#?}", ts.to_string());
    ts.into()
}
