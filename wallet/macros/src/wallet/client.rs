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
    // rpc_api_ops: Expr,
    handlers: ExprArray,
}

impl Parse for RpcTable {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 1 {
            return Err(Error::new_spanned(parsed, "usage: build_wrpc_client_interface!([XxxRequest, ..])".to_string()));
        }

        let mut iter = parsed.iter();
        // Intake the enum name
        // let rpc_api_ops = iter.next().unwrap().clone();
        // Intake enum variants as an array
        let handlers = get_handlers(iter.next().unwrap().clone())?;

        Ok(RpcTable { handlers })
    }
}

impl ToTokens for RpcTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets = Vec::new();
        // let rpc_api_ops = &self.rpc_api_ops;

        for handler in self.handlers.elems.iter() {
            let Handler { hash_64, ident, fn_call, request_type, response_type, .. } = Handler::new(handler);

            targets.push(quote! {
                fn #fn_call<'async_trait>(
                    self: Arc<Self>,
                    request: #request_type,
                ) -> ::core::pin::Pin<
                    Box<dyn ::core::future::Future<Output = Result<#response_type>> + ::core::marker::Send + 'async_trait>,
                >
                where
                    Self: 'async_trait,
                {
                    Box::pin(async move {
                        if let ::core::option::Option::Some(__ret) = ::core::option::Option::None::<Result<#response_type>> {
                            return __ret;
                        }
                        let op: u64 = #hash_64;
                        let __self = self;
                        let request = request;
                        let __ret: Result<#response_type> =
                            {
                                match __self.codec {
                                    Codec::Borsh(ref codec) => {
                                        Ok(#response_type::try_from_slice(&codec.call(op, borsh::to_vec(&request)?).await?)?)
                                    },
                                    Codec::Serde(ref codec) => {
                                        let request = serde_json::to_string(&request)?;
                                        let response = codec.call(#ident, request.as_str()).await?;
                                        Ok(serde_json::from_str::<#response_type>(response.as_str())?)
                                    },
                                }
                                // Ok(#response_type::try_from_slice(&__self.codec.call(op, &request.try_to_vec()?).await?)?)
                            };
                        #[allow(unreachable_code)]
                        __ret
                    })
                }

            });
        }

        quote! {
            #(#targets)*
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
