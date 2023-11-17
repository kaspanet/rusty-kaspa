use downcast::DowncastError;
use kaspa_daemon::error::Error as DaemonError;
use thiserror::Error;
use wasm_bindgen::JsValue;
use workflow_core::channel::ChannelError;
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
    Cli(#[from] kaspa_cli_lib::error::Error),

    #[error(transparent)]
    Ipc(#[from] workflow_nw::ipc::error::Error),

    #[error("{0}")]
    JsValue(Printable),

    #[error("{0}")]
    Terminal(#[from] workflow_terminal::error::Error),

    #[error("channel error")]
    RecvError(#[from] workflow_core::channel::RecvError),

    #[error(transparent)]
    CallbackError(#[from] workflow_wasm::callback::CallbackError),

    #[error("{0}")]
    DowncastError(String),

    #[error("Channel error")]
    ChannelError(String),

    #[error(transparent)]
    Daemon(#[from] kaspa_daemon::error::Error),

    #[error(transparent)]
    WalletError(#[from] kaspa_wallet_core::error::Error),

    #[error(transparent)]
    Dom(#[from] workflow_dom::error::Error),

    #[error(transparent)]
    D3(#[from] workflow_d3::error::Error),
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

impl<T> From<DowncastError<T>> for Error {
    fn from(e: DowncastError<T>) -> Self {
        Error::DowncastError(e.to_string())
    }
}

impl<T> From<ChannelError<T>> for Error {
    fn from(e: ChannelError<T>) -> Error {
        Error::ChannelError(e.to_string())
    }
}
