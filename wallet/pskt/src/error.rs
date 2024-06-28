#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ConstructorError(#[from] ConstructorError),
    #[error("OutputNotModifiable")]
    OutOfBounds,
    #[error("Missing UTXO entry")]
    MissingUtxoEntry,
    #[error(transparent)]
    InputBuilder(#[from] crate::input::InputBuilderError),
    #[error(transparent)]
    OutputBuilder(#[from] crate::output::OutputBuilderError),
}

#[derive(thiserror::Error, Debug)]
pub enum ConstructorError {
    #[error("InputNotModifiable")]
    InputNotModifiable,
    #[error("OutputNotModifiable")]
    OutputNotModifiable,
}
