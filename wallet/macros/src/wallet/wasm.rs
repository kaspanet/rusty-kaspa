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
struct RpcHandlers {
    handlers_no_args: ExprArray,
    handlers_with_args: ExprArray,
}

impl Parse for RpcHandlers {
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

        let handlers = RpcHandlers { handlers_no_args, handlers_with_args };
        Ok(handlers)
    }
}

impl ToTokens for RpcHandlers {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets_no_args = Vec::new();
        let mut targets_with_args = Vec::new();

        for handler in self.handlers_no_args.elems.iter() {
            let Handler { fn_call, fn_camel, fn_no_suffix, request_type, response_type, .. } = Handler::new(handler);

            targets_no_args.push(quote! {

                #[wasm_bindgen(js_name = #fn_camel)]
                pub async fn #fn_no_suffix(&self) -> Result<JsValue> {
                    let value: JsValue = js_sys::Object::new().into();
                    let request: #request_type = from_value(value)?;
                    // log_info!("request: {:#?}",request);
                    let result: RpcResult<#response_type> = self.client.#fn_call(request).await;
                    // log_info!("result: {:#?}",result);

                    let response: #response_type = result.map_err(|err|wasm_bindgen::JsError::new(&err.to_string()))?;
                    //log_info!("response: {:#?}",response);
                    workflow_wasm::serde::to_value(&response).map_err(|err|err.into())
                }

            });
        }

        for handler in self.handlers_with_args.elems.iter() {
            let Handler { fn_call, fn_camel, fn_no_suffix, request_type, response_type, .. } = Handler::new(handler);

            targets_with_args.push(quote! {

                #[wasm_bindgen(js_name = #fn_camel)]
                pub async fn #fn_no_suffix(&self, request: JsValue) -> Result<JsValue> {
                    let request: #request_type = from_value(request)?;
                    let result: RpcResult<#response_type> = self.client.#fn_call(request).await;
                    let response: #response_type = result.map_err(|err|wasm_bindgen::JsError::new(&err.to_string()))?;
                    workflow_wasm::serde::to_value(&response).map_err(|err|err.into())
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
    let rpc_table = parse_macro_input!(input as RpcHandlers);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}

// #####################################################################

#[derive(Debug)]
struct RpcSubscriptions {
    handlers: ExprArray,
}

impl Parse for RpcSubscriptions {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 1 {
            return Err(Error::new_spanned(
                parsed,
                "usage: build_wrpc_wasm_bindgen_interface!([fn no args, ..],[fn with args, ..])".to_string(),
            ));
        }

        let mut iter = parsed.iter();
        let handlers = get_handlers(iter.next().unwrap().clone())?;

        Ok(RpcSubscriptions { handlers })
    }
}

impl ToTokens for RpcSubscriptions {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets = Vec::new();

        for handler in self.handlers.elems.iter() {
            let name = format!("Notify{}", handler.to_token_stream().to_string().as_str());
            let regex = Regex::new(r"^Notify").unwrap();
            let blank = regex.replace(&name, "");
            let subscribe = regex.replace(&name, "Subscribe");
            let unsubscribe = regex.replace(&name, "Unsubscribe");
            let scope = Ident::new(&blank, Span::call_site());
            let sub_scope = Ident::new(format!("{blank}Scope").as_str(), Span::call_site());
            let fn_subscribe_snake = Ident::new(&subscribe.to_case(Case::Snake), Span::call_site());
            let fn_subscribe_camel = Ident::new(&subscribe.to_case(Case::Camel), Span::call_site());
            let fn_unsubscribe_snake = Ident::new(&unsubscribe.to_case(Case::Snake), Span::call_site());
            let fn_unsubscribe_camel = Ident::new(&unsubscribe.to_case(Case::Camel), Span::call_site());

            targets.push(quote! {

                #[wasm_bindgen(js_name = #fn_subscribe_camel)]
                pub async fn #fn_subscribe_snake(&self) -> Result<()> {
                    self.client.start_notify(ListenerId::default(), Scope::#scope(#sub_scope {})).await?;
                    Ok(())
                }

                #[wasm_bindgen(js_name = #fn_unsubscribe_camel)]
                pub async fn #fn_unsubscribe_snake(&self) -> Result<()> {
                    self.client.stop_notify(ListenerId::default(), Scope::#scope(#sub_scope {})).await?;
                    Ok(())
                }

            });
        }

        quote! {
            #[wasm_bindgen]
            impl RpcClient {
                #(#targets)*
            }
        }
        .to_tokens(tokens);
    }
}

pub fn build_wrpc_wasm_bindgen_subscriptions(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcSubscriptions);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}
