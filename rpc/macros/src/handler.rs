use convert_case::{Case, Casing};
use proc_macro2::{Ident, Span};
use quote::ToTokens;
use syn::{Attribute, Error, Expr, ExprArray, Result};

#[derive(Debug)]
pub struct Handler {
    pub name: String,
    pub fn_call: Ident,
    pub fn_with_suffix: Option<Ident>,
    pub fn_no_suffix: Ident,
    pub fn_camel: Ident,
    pub request_type: Ident,
    pub response_type: Ident,
    pub typename: Ident,
    pub ts_request_type: Ident,
    pub ts_response_type: Ident,
    pub ts_custom_section_ident: Ident,

    // gPRC fields
    pub is_subscription: bool,
    pub response_message_type: Ident,
    pub fallback_request_type: Ident,
    pub docs: Vec<Attribute>,
}

impl Handler {
    pub fn new(handler: &Expr) -> Handler {
        Handler::new_with_args(handler, None)
    }

    pub fn new_with_args(handler: &Expr, fn_suffix: Option<&str>) -> Handler {
        let (name, docs) = match handler {
            syn::Expr::Path(expr_path) => (expr_path.path.to_token_stream().to_string(), expr_path.attrs.clone()),
            _ => (handler.to_token_stream().to_string(), vec![]),
        };
        //let name = handler.to_token_stream().to_string();
        let fn_call = Ident::new(&format!("{}_call", name.to_case(Case::Snake)), Span::call_site());
        let fn_with_suffix = fn_suffix.map(|suffix| Ident::new(&format!("{}_{suffix}", name.to_case(Case::Snake)), Span::call_site()));
        let fn_no_suffix = Ident::new(&name.to_case(Case::Snake), Span::call_site());
        let fn_camel = Ident::new(&name.to_case(Case::Camel), Span::call_site());
        let request_type = Ident::new(&format!("{name}Request"), Span::call_site());
        let response_type = Ident::new(&format!("{name}Response"), Span::call_site());
        let typename = Ident::new(&name.to_string(), Span::call_site());
        let ts_request_type = Ident::new(&format!("I{name}Request"), Span::call_site());
        let ts_response_type = Ident::new(&format!("I{name}Response"), Span::call_site());
        let ts_custom_section_ident = Ident::new(&format!("TS_{}", name.to_uppercase()), Span::call_site());

        // gPRC fields
        let fallback_name = name.replace("StopNotifying", "Notify");
        let is_subscription = fallback_name.starts_with("Notify");
        let response_message_type = Ident::new(&format!("{name}ResponseMessage"), Span::call_site());
        let fallback_request_type = Ident::new(&format!("{fallback_name}Request"), Span::call_site());
        Handler {
            name,
            fn_call,
            fn_with_suffix,
            fn_no_suffix,
            fn_camel,
            request_type,
            response_type,
            typename,
            ts_request_type,
            ts_response_type,
            ts_custom_section_ident,
            is_subscription,
            response_message_type,
            fallback_request_type,
            docs,
        }
    }
}

pub fn get_handlers(handlers: Expr) -> Result<ExprArray> {
    let handlers = match handlers {
        Expr::Array(array) => array,
        _ => {
            return Err(Error::new_spanned(handlers, "the argument must be an array of enum variants".to_string()));
        }
    };

    for ph in handlers.elems.iter() {
        match ph {
            Expr::Path(_exp_path) => {}
            _ => {
                return Err(Error::new_spanned(ph, "handlers should contain enum variants".to_string()));
            }
        }
    }

    Ok(handlers)
}
