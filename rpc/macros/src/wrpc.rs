use convert_case::{Case, Casing};
use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use std::convert::Into;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Error, Expr, ExprArray, ExprPath, Result, Token,
};

#[derive(Debug)]
struct RpcTable {
    server_ctx: Expr,
    router_target: Expr,
    // field: Expr,
    server_ctx_type: Expr,
    connection_ctx_type: Expr,
    // rpc_api_ops: ExprPath,
    rpc_api_ops: Expr,
    handlers: ExprArray,
}

impl Parse for RpcTable {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 6 {
            return Err(Error::new_spanned(parsed, "usage: build_wrpc_interface!(interface, RpcApiOps,[getInfo, ..])".to_string()));
        }

        let mut iter = parsed.iter();
        let server_ctx = iter.next().unwrap().clone();
        let router_target = iter.next().unwrap().clone();
        // let field = iter.next().unwrap().clone();
        let server_ctx_type = iter.next().unwrap().clone();
        let connection_ctx_type = iter.next().unwrap().clone();
        // let server_ctx_type = match server_ctx_type_expr {
        //     Expr::Path(path) => path,
        //     _ => {
        //         return Err(Error::new_spanned(server_ctx_type_expr, "the first argument should be the Ops enum)".to_string()));
        //     }
        // };

        // let mut iter = parsed.iter();
        // let rpc_api_ops_expr = iter.next().unwrap().clone();
        // let rpc_api_ops = match rpc_api_ops_expr {
        //     Expr::Path(ident) => ident,
        //     _ => {
        //         return Err(Error::new_spanned(rpc_api_ops_expr, "the first argument should be the Ops enum)".to_string()));
        //     }
        // };

        // let mut iter = parsed.iter();
        let rpc_api_ops = iter.next().unwrap().clone();
        // let rpc_api_ops_expr = iter.next().unwrap().clone();
        // let rpc_api_ops = match rpc_api_ops_expr {
        //     Expr::Path(ident) => ident,
        //     _ => {
        //         return Err(Error::new_spanned(rpc_api_ops_expr, "the first argument should be the Ops enum)".to_string()));
        //     }
        // };

        let handlers_ = iter.next().unwrap().clone();
        let mut handlers = match handlers_ {
            Expr::Array(array) => array,
            _ => {
                return Err(Error::new_spanned(handlers_, "the second argument must be an array of enum values".to_string()));
            }
        };

        for ph in handlers.elems.iter_mut() {
            match ph {
                Expr::Path(_exp_path) => {}
                _ => {
                    return Err(Error::new_spanned(ph, "handlers should contain a paths to enum variants".to_string()));
                }
            }
        }

        let handlers = RpcTable { 
            server_ctx,
            router_target,
            // field,
            server_ctx_type,
            connection_ctx_type,
            rpc_api_ops,
            handlers,
        };
        Ok(handlers)
    }
}

impl ToTokens for RpcTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        //println!("rpc_table: {:#?}", rpc_table);

        let mut server_targets = Vec::new();
        let mut connection_targets = Vec::new();
        let server_ctx = &self.server_ctx;
        // let field = &self.field;
        let router_target = &self.router_target;
        let server_ctx_type = &self.server_ctx_type;
        let connection_ctx_type = &self.connection_ctx_type;
        let rpc_api_ops = &self.rpc_api_ops;

        for handler in self.handlers.elems.iter() {
            let name = handler.to_token_stream().to_string();
            let fn_call = Ident::new(&format!("{}_call", name.to_case(Case::Snake)), Span::call_site());
            let request_type = Ident::new(&format!("{name}Request"), Span::call_site());
            let response_type = Ident::new(&format!("{name}Response"), Span::call_site());

            server_targets.push(quote! {
                #rpc_api_ops::#handler => {
                    interface.method(#rpc_api_ops::#handler, method!(|server_ctx: #server_ctx_type, connection_ctx: #connection_ctx_type, request: #request_type| async move {
                        let v: #response_type = server_ctx.get_rpc_api().#fn_call(request)
                            .map_err(|e|ServerError::Text(e.to_string()))?;
                        Ok(v)
                    }));
                }
            });

            connection_targets.push(quote! {
                #rpc_api_ops::#handler => {
                    interface.method(#rpc_api_ops::#handler, method!(|server_ctx: #server_ctx_type, connection_ctx: #connection_ctx_type, request: #request_type| async move {
                        let v: #response_type = connection_ctx.get_rpc_api().#fn_call(request)
                        .map_err(|e|ServerError::Text(e.to_string()))?;
                        Ok(v)
                    }));
                }
            });
        }

        quote! {

            {
                let mut interface = workflow_rpc::server::Interface<
                    #server_ctx_type,
                    #connection_ctx_type,
                    #rpc_api_ops
                >::new(#server_ctx);

                match #router_target {
                    RouterTarget::Server => {
                        for op in #rpc_api_ops::list() {
                            match op {
                                #(#server_targets)*
                            }
                        }
                    },
                    RouterTarget::Client => {
                        for op in #rpc_api_ops::list() {
                            match op {
                                #(#connection_targets)*
                            }
                        }
                    }
                }

                Arc::new(interface)
            }



        }
        .to_tokens(tokens);
    }
}

pub fn build_wrpc_interface(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcTable);
    let ts = rpc_table.to_token_stream();
    println!("ts====>: {:#?}", ts.to_string());
    ts.into()
}
