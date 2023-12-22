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
    kaspad_request_type: Expr,
    kaspad_response_type: Expr,
    payload_ops: Expr,
    handlers: ExprArray,
}

impl Parse for RpcTable {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();
        if parsed.len() != 7 {
            return Err(Error::new_spanned(parsed,
                "usage: build_grpc_server_interface!(server_context, ServerContextType, ConnectionType, KaspadRequestType, KaspadResponseType, KaspadPayloadOps, [GetInfo, ..])".to_string()));
        }

        let mut iter = parsed.iter();
        let server_ctx = iter.next().unwrap().clone();
        let server_ctx_type = iter.next().unwrap().clone();
        let connection_ctx_type = iter.next().unwrap().clone();
        let kaspad_request_type = iter.next().unwrap().clone();
        let kaspad_response_type = iter.next().unwrap().clone();
        let payload_ops = iter.next().unwrap().clone();
        let handlers = get_handlers(iter.next().unwrap().clone())?;

        Ok(RpcTable {
            server_ctx,
            server_ctx_type,
            connection_ctx_type,
            kaspad_request_type,
            kaspad_response_type,
            payload_ops,
            handlers,
        })
    }
}

impl ToTokens for RpcTable {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets = Vec::new();
        let server_ctx = &self.server_ctx;
        let server_ctx_type = &self.server_ctx_type;
        let connection_ctx_type = &self.connection_ctx_type;
        let kaspad_request_type = &self.kaspad_request_type;
        let kaspad_response_type = &self.kaspad_response_type;
        let payload_ops = &self.payload_ops;

        for handler in self.handlers.elems.iter() {
            let Handler { fn_call, request_type, is_subscription, response_message_type, fallback_request_type, .. } =
                Handler::new(handler);

            match is_subscription {
                false => {
                    targets.push(quote! {
                        #payload_ops::#handler => {
                            let method: Method<#server_ctx_type, #connection_ctx_type, #kaspad_request_type, #kaspad_response_type> =
                            Method::new(|server_ctx: #server_ctx_type, _: #connection_ctx_type, request: #kaspad_request_type| {
                                Box::pin(async move {
                                    let mut response: #kaspad_response_type = match request.payload {
                                        Some(Payload::#request_type(ref request)) => match request.try_into() {
                                            Ok(request) => server_ctx.core_service.#fn_call(request).await.into(),
                                            Err(err) => #response_message_type::from(err).into(),
                                        },
                                        _ => {
                                            return Err(GrpcServerError::InvalidRequestPayload);
                                        }
                                    };
                                    response.id = request.id;
                                    Ok(response)
                                })
                            });
                            interface.method(#payload_ops::#handler, method);
                        }
                    });
                }
                true => {
                    targets.push(quote! {
                        #payload_ops::#handler => {
                            let method: Method<#server_ctx_type, #connection_ctx_type, #kaspad_request_type, #kaspad_response_type> =
                            Method::new(|server_ctx: #server_ctx_type, connection: #connection_ctx_type, request: #kaspad_request_type| {
                                Box::pin(async move {
                                    let mut response: #kaspad_response_type = match request.payload {
                                        Some(Payload::#request_type(ref request)) => {
                                            match kaspa_rpc_core::#fallback_request_type::try_from(request) {
                                                Ok(request) => {
                                                    let listener_id = connection.get_or_register_listener_id()?;
                                                    let command = request.command;
                                                    let result = server_ctx
                                                        .notifier
                                                        .clone()
                                                        .execute_subscribe_command(listener_id, request.into(), command)
                                                        .await;
                                                    #response_message_type::from(result).into()
                                                }
                                                Err(err) => #response_message_type::from(err).into(),
                                            }
                                        }
                                        _ => {
                                            return Err(GrpcServerError::InvalidRequestPayload);
                                        }
                                    };
                                    response.id = request.id;
                                    Ok(response)
                                })
                            });
                            interface.method(#payload_ops::#handler, method);
                        }
                    });
                }
            }
        }

        quote! {
            {
                let mut interface = Interface::new(#server_ctx);

                for op in #payload_ops::list() {
                    match op {
                        #(#targets)*
                    }
                }

                interface
            }
        }
        .to_tokens(tokens);
    }
}

pub fn build_grpc_server_interface(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let rpc_table = parse_macro_input!(input as RpcTable);
    let ts = rpc_table.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}
