use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),
}

impl Error {
    pub fn custom<T: ToString>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}
