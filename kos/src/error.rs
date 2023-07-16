use kaspa_daemon::error::Error as DaemonError;
use thiserror::Error;
use wasm_bindgen::JsValue;
use workflow_nw::ipc::ResponseError;
use workflow_wasm::printable::Printable;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    WorkflowNw(#[from] workflow_nw::error::Error),

    #[error(transparent)]
    KaspaWalletCli(#[from] kaspa_cli::error::Error),

    #[error(transparent)]
    Ipc(#[from] workflow_nw::ipc::error::Error),

    #[error("{0}")]
    JsValue(Printable),

    #[error("{0}")]
    Terminal(#[from] workflow_terminal::error::Error),

    #[error("channel error")]
    RecvError(#[from] workflow_core::channel::RecvError),
}

impl From<Error> for JsValue {
    fn from(err: Error) -> JsValue {
        let s: String = err.to_string();
        JsValue::from_str(&s)
    }
}

impl From<JsValue> for Error {
    fn from(js_value: JsValue) -> Error {
        Error::JsValue(Printable::new(js_value))
    }
}

impl From<Error> for ResponseError {
    fn from(err: Error) -> ResponseError {
        ResponseError::Custom(err.to_string())
    }
}

impl From<Error> for DaemonError {
    fn from(err: Error) -> DaemonError {
        DaemonError::Custom(err.to_string())
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::Custom(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self::Custom(err.to_string())
    }
}
