use crate::handler::*;
use convert_case::{Case, Casing};
use proc_macro2::{Literal, TokenStream};
use quote::{quote, ToTokens};
use std::convert::Into;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Error, Expr, ExprArray, ExprLit, Lit, Result, Token,
};

#[derive(Debug)]
struct TsInterface {
    handler: Handler,
    alias: Literal,
    declaration: String,
}

impl Parse for TsInterface {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();

        if parsed.len() == 2 {
            let mut iter = parsed.iter();
            let handler = Handler::new(iter.next().unwrap());
            let alias = Literal::string(&handler.name);
            let declaration = extract_literal(&iter.next().unwrap().clone())?;
            Ok(TsInterface { handler, alias, declaration })
        } else if parsed.len() == 3 {
            let mut iter = parsed.iter();
            let handler = Handler::new(iter.next().unwrap());
            let alias = match iter.next().unwrap().clone() {
                Expr::Lit(ExprLit { lit: Lit::Str(lit_str), .. }) => Literal::string(&lit_str.value()),
                _ => return Err(Error::new_spanned(parsed, "type spec must be a string literal".to_string())),
            };
            let declaration = extract_literal(&iter.next().unwrap().clone())?;
            Ok(TsInterface { handler, alias, declaration })
        } else {
            return Err(Error::new_spanned(
                parsed,
                "usage: declare_wasm_interface!(typescript_type, [alias], typescript declaration)".to_string(),
            ));
        }
    }
}

impl ToTokens for TsInterface {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self { handler, alias, declaration } = self;
        let Handler { name, typename, ts_custom_section_ident, .. } = handler;

        let declaration = if name.ends_with("Request") {
            let method = (&name.trim_end_matches("Request")[1..]).to_case(Case::Camel);
            insert_typedoc(
                declaration,
                &format!(
                    r#"
                Argument interface for the {{@link Wallet.{method}}} method.
            "#
                ),
            )
        } else if name.ends_with("Response") {
            let method = (&name.trim_end_matches("Response")[1..]).to_case(Case::Camel);
            insert_typedoc(
                declaration,
                &format!(
                    r#"
                Return interface for the {{@link Wallet.{method}}} method.
            "#
                ),
            )
        } else {
            declaration.to_owned()
        };

        quote! {

            #[wasm_bindgen(typescript_custom_section)]
            const #ts_custom_section_ident: &'static str = #declaration;

            #[wasm_bindgen]
            extern "C" {
                #[wasm_bindgen(extends = js_sys::Object, typescript_type = #alias)]
                #[derive(Default)]
                pub type #typename;
            }


        }
        .to_tokens(tokens);
    }
}

pub fn declare_typescript_wasm_interface(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let declaration = parse_macro_input!(input as TsInterface);
    let ts = declaration.to_token_stream();
    // println!("MACRO: {}", ts.to_string());
    ts.into()
}

// #####################################################################

// DECLARE WALLET FUNCTIONS

#[derive(Debug)]
struct ApiHandlers {
    handlers: ExprArray,
    // handlers_with_args: ExprArray,
    // handler : Handler,
}

impl Parse for ApiHandlers {
    fn parse(input: ParseStream) -> Result<Self> {
        let parsed = Punctuated::<Expr, Token![,]>::parse_terminated(input).unwrap();

        // ApiName (WalletOpen)
        // Typescript Type Body ( arg : Something | undefined )
        // Request TryFrom body ( TryFrom<IWalletOpenRequest> )
        // Response TryFrom body ( TryFrom<IWalletOpenResponse>)

        if parsed.len() != 1 {
            return Err(Error::new_spanned(
                parsed,
                "usage: build_wrpc_wasm_bindgen_interface!([fn no args, ..],[fn with args, ..])".to_string(),
            ));
        }

        let mut iter = parsed.iter();
        // let handler = Handler::new(iter.next().unwrap());
        let handlers = get_handlers(iter.next().unwrap().clone())?;
        // let handlers_with_args = get_handlers(iter.next().unwrap().clone())?;

        // let handlers = ApiInterface { handlers_no_args, handlers_with_args };
        Ok(ApiHandlers { handlers })
    }
}

impl ToTokens for ApiHandlers {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut targets = Vec::new();

        for handler in self.handlers.elems.iter() {
            let Handler { fn_call, fn_camel, fn_no_suffix, request_type, ts_request_type, ts_response_type, docs, .. } =
                Handler::new(handler);
            let links = format! {"@see {{@link {ts_request_type}}} {{@link {ts_response_type}}}"};
            let throws = "@throws `string` in case of an error.";
            targets.push(quote! {
                #(#docs)*
                #[doc=#links]
                #[doc=#throws]
                #[wasm_bindgen(js_name = #fn_camel)]
                pub async fn #fn_no_suffix(&self, request : #ts_request_type) -> Result<#ts_response_type> {
                    let request = #request_type::try_from(request)?;
                    let response = self.wallet.clone().#fn_call(request).await?;
                    #ts_response_type::try_from(response)
                }

            });
        }
        quote! {
            #[wasm_bindgen]
            impl Wallet {
                #(#targets)*
            }
        }
        .to_tokens(tokens);
    }
}

pub fn declare_wasm_handlers(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let declaration = parse_macro_input!(input as ApiHandlers);
    let ts = declaration.to_token_stream();
    // println!("MACRO: {}", ts);
    ts.into()
}

// #####################################################################
/*
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
*/

fn extract_literal(expr: &Expr) -> Result<String> {
    match expr {
        Expr::Lit(expr_lit) => {
            if let Lit::Str(lit_str) = &expr_lit.lit {
                Ok(lit_str.value())
            } else {
                Err(Error::new_spanned(expr, "argument must be a string literal".to_string()))
            }
        }
        _ => Err(Error::new_spanned(expr, "argument must be a string literal".to_string())),
    }
}

fn insert_typedoc(text: &str, insertion: &str) -> String {
    if let Some(mut index) = text.find("/**") {
        index += 3;
        let insertion = insertion
            .split('\n')
            .filter_map(|line| (!line.trim().is_empty()).then_some(format!("\n\t* {}", line.trim())))
            .collect::<String>();
        let mut result = String::with_capacity(text.len() + insertion.len());
        result.push_str(&text[..index]);
        result.push_str(&insertion);
        result.push_str(&text[index..]);

        let lines = result
            .split('\n')
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("/**") || trimmed.starts_with('*') {
                    trimmed
                } else {
                    line
                }
            })
            .collect::<Vec<&str>>()
            .join("\n");

        lines
    } else {
        text.to_string()
    }
}
