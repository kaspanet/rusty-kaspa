use convert_case::{Case, Casing};
use proc_macro2::{Ident, Span, TokenStream};
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
        // let router_target = iter.next().unwrap().clone();
        let server_ctx_type = iter.next().unwrap().clone();
        let connection_ctx_type = iter.next().unwrap().clone();
        let rpc_api_ops = iter.next().unwrap().clone();

        let handlers_ = iter.next().unwrap().clone();
        let mut handlers = match handlers_ {
            Expr::Array(array) => array,
            _ => {
                return Err(Error::new_spanned(handlers_, "last argument must be an array of enum values".to_string()));
            }
        };

        for ph in handlers.elems.iter_mut() {
            match ph {
                Expr::Path(_exp_path) => {}
                _ => {
                    return Err(Error::new_spanned(ph, "handlers should contain enum variants".to_string()));
                }
            }
        }

        let handlers = RpcTable { server_ctx, server_ctx_type, connection_ctx_type, rpc_api_ops, handlers };
        Ok(handlers)
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
            let name = handler.to_token_stream().to_string();
            let fn_call = Ident::new(&format!("{}_call", name.to_case(Case::Snake)), Span::call_site());
            let request_type = Ident::new(&format!("{name}Request"), Span::call_site());
            let response_type = Ident::new(&format!("{name}Response"), Span::call_site());

            targets.push(quote! {
                #rpc_api_ops::#handler => {
                    interface.method(#rpc_api_ops::#handler, method!(|server_ctx: #server_ctx_type, connection_ctx: #connection_ctx_type, request: #request_type| async move {
                        let verbose = server_ctx.verbose();
                        if verbose { workflow_log::log_info!("rpc request: {:?}",request); }
                        let response: #response_type = server_ctx.get_rpc_api(&connection_ctx).#fn_call(request).await
                            .map_err(|e|ServerError::Text(e.to_string()))?;
                        workflow_log::log_trace!("rpc response: {:?}",response);
                        if verbose { workflow_log::log_info!("rpc response: {:?}",response); }
                        Ok(response)
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

                for op in #rpc_api_ops::list() {
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
    // println!("ts====>: {:#?}", ts.to_string());
    ts.into()
}
