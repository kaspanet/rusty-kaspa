use convert_case::{Case, Casing};
use proc_macro2::{Ident, Span};
use quote::ToTokens;
use syn::{Error, Expr, ExprArray, Result};

pub struct Handler {
    pub name: String,
    pub fn_call: Ident,
    pub fn_with_suffix: Option<Ident>,
    pub fn_no_suffix: Ident,
    pub request_type: Ident,
    pub response_type: Ident,
}

impl Handler {
    pub fn new(handler: &Expr) -> Handler {
        Handler::new_with_args(handler, None)
    }

    pub fn new_with_args(handler: &Expr, fn_suffix: Option<&str>) -> Handler {
        let name = handler.to_token_stream().to_string();
        let fn_call = Ident::new(&format!("{}_call", name.to_case(Case::Snake)), Span::call_site());
        let fn_with_suffix = fn_suffix.map(|suffix| Ident::new(&format!("{}_{suffix}", name.to_case(Case::Snake)), Span::call_site()));
        let fn_no_suffix = Ident::new(&name.to_case(Case::Snake), Span::call_site());
        let request_type = Ident::new(&format!("{name}Request"), Span::call_site());
        let response_type = Ident::new(&format!("{name}Response"), Span::call_site());
        Handler { name, fn_call, fn_with_suffix, fn_no_suffix, request_type, response_type }
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
