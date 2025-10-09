use std::error::Error;
use thiserror::Error;

use crate::login;

#[derive(Debug, Error)]
pub enum LoginResponseError {
    #[error("Failed to convert XMLRPC {0}")]
    XLMRPCError(#[from] Box<dyn Error + Send + Sync>),

    #[error("failed to deserialize XMLRPC: {0}")]
    DeserializeError(#[from] std::io::Error),

    #[error("Failed to convert {0}")]
    ConversionError(#[from] login::login_errors::ConversionError),
}
