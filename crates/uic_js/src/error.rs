//! The host's error type: JavaScript failures carry their engine message.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("javascript error: {0}")]
    Js(String),
    #[error("unknown module specifier {0}")]
    UnknownModule(String),
    #[cfg(feature = "sqlite")]
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<boa_engine::JsError> for Error {
    fn from(err: boa_engine::JsError) -> Self {
        Error::Js(err.to_string())
    }
}
