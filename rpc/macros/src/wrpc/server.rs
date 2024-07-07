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
    server_ctx: Expr,
    server_ctx_type: Expr,
    connection_ctx_type: Expr,
    rpc_api_ops: Expr,
    handlers: ExprArray,
}

impl Parse for RpcTable {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 5 {
            return Err(Error::new_spanned(parsed,
                "usage: build_wrpc_server_interface!(server_instance,router_instance,ServerType,ConnectionType,RpcApiOps,[getInfo, ..])".to_string()));
        }

        let mut iter = parsed.iter();
        let server_ctx = iter.next().unwrap().clone();
        let server_ctx_type = iter.next().unwrap().clone();
        let connection_ctx_type = iter.next().unwrap().clone();
        let rpc_api_ops = iter.next().unwrap().clone();
        let handlers = get_handlers(iter.next().unwrap().clone())?;

        Ok(RpcTable { server_ctx, server_ctx_type, connection_ctx_type, rpc_api_ops, handlers })
    }
}

impl ToTokens for RpcTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets = Vec::new();
        let server_ctx = &self.server_ctx;
        let server_ctx_type = &self.server_ctx_type;
        let connection_ctx_type = &self.connection_ctx_type;
        let rpc_api_ops = &self.rpc_api_ops;

        for handler in self.handlers.elems.iter() {
            let Handler { fn_call, request_type, response_type, .. } = Handler::new(handler);

            targets.push(quote! {
                #rpc_api_ops::#handler => {
                    interface.method(#rpc_api_ops::#handler, method!(|server_ctx: #server_ctx_type, connection_ctx: #connection_ctx_type, request: Serializable<#request_type>| async move {
                        let verbose = server_ctx.verbose();
                        if verbose { workflow_log::log_info!("request: {:?}",request); }
                        // TODO: RPC-CONNECT
                        let response: #response_type = server_ctx.rpc_service(&connection_ctx).#fn_call(None, request.into_inner()).await
                            .map_err(|e|ServerError::Text(e.to_string()))?;
                        if verbose { workflow_log::log_info!("response: {:?}",response); }
                        Ok(Serializable(response))
                    }));
                }
            });
        }

        quote! {

            {
                let mut interface = workflow_rpc::server::Interface::<
                    #server_ctx_type,
                    #connection_ctx_type,
                    #rpc_api_ops
                >::new(#server_ctx);

                for op in #rpc_api_ops::iter() {
                    use workflow_serializer::prelude::*;
                    match op {
                        #(#targets)*
                        _ => { }
                    }
                }

                interface
            }
        }
        .to_tokens(tokens);
    }
}

pub fn build_wrpc_server_interface(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcTable);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}
