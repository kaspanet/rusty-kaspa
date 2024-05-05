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
            return Err(Error::new_spanned(parsed, "usage: build_wrpc_server_interface!([XxxOp, ..])".to_string()));
        }

        let mut iter = parsed.iter();
        let handlers = get_handlers(iter.next().unwrap().clone())?;

        Ok(RpcTable { handlers })
    }
}

impl ToTokens for RpcTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets_borsh = Vec::new();
        let mut targets_serde = Vec::new();

        for handler in self.handlers.elems.iter() {
            let Handler { hash_64, ident, fn_call, request_type, .. } = Handler::new(handler);

            targets_borsh.push(quote! {
                #hash_64 => {
                    Ok(self.wallet_api().#fn_call(#request_type::try_from_slice(&request)?).await?.try_to_vec()?)
                }
            });

            targets_serde.push(quote! {
                #ident => {
                    let request: #request_type = serde_json::from_str(request)?;
                    let response = self.wallet_api().#fn_call(request).await?;
                    Ok(serde_json::to_string(&response)?)
                }
            });

            // targets_serde_wasm.push(quote! {
            //     #ident => {
            //         Ok(self.wallet_api.clone().#fn_call(#request_type::try_from_slice(&request)?).await?.try_to_vec()?)
            //     }
            // });
        }

        quote! {

                pub async fn call_with_borsh(&self, op: u64, request: &[u8]) -> Result<Vec<u8>> {
                    match op {
                        #(#targets_borsh)*
                        _ => { Err(Error::NotImplemented) }
                    }
                }

                pub async fn call_with_serde(&self, op: &str, request: &str) -> Result<String> {
                    match op {
                        #(#targets_serde)*
                        _ => { Err(Error::NotImplemented) }
                    }
                }

                // async fn call_with_serde_wasm(&self, op: &str, request : &JsValue) -> Result<JsValue> {
                //     match op {
                //         #(#targets_serde_wasm)*
                //         _ => { Err(Error::NotImplemented) }
                //     }
                // }

        }
        .to_tokens(tokens);
    }
}

pub fn build_transport_interface(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcTable);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}
