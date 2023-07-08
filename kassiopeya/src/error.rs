use thiserror::Error;
use wasm_bindgen::JsValue;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    WorkflowNw(#[from] workflow_nw::error::Error),
    #[error(transparent)]
    KaspaWalletCli(#[from] kaspa_cli::error::Error),
}

impl From<Error> for JsValue {
    fn from(err: Error) -> JsValue {
        let s: String = err.to_string();
        JsValue::from_str(&s)
    }
}
