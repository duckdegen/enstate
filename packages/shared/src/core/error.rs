use crate::core::address::AddressResolveError;
use crate::core::gns::NameError;
use crate::core::resolvers::gns::GnsError;
use ethers::prelude::{Provider, ProviderError};
use ethers::providers::Http;
use ethers_ccip_read::CCIPReadMiddlewareError;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProfileError {
    #[error("Not Found")]
    NotFound,

    #[error("{0}")]
    InvalidName(#[from] NameError),

    #[error("Address resolve error: {0}")]
    AddressResolveError(#[from] AddressResolveError),

    #[error("RPC error: {0}")]
    RPCError(#[from] ProviderError),

    #[error("CCIP error: {0}")]
    CCIPError(#[from] CCIPReadMiddlewareError<Arc<Provider<Http>>>),

    #[error("DNS encode error: {0}")]
    DNSEncodeError(String),

    #[error("Implementation error: {0}")]
    ImplementationError(String),

    #[error("Other: {0}")]
    Other(String),
}

impl From<GnsError> for ProfileError {
    fn from(error: GnsError) -> Self {
        match error {
            GnsError::Rpc(error) => Self::RPCError(error),
            GnsError::Decode(_) => Self::ImplementationError(error.to_string()),
        }
    }
}

impl AsRef<ProfileError> for ProfileError {
    fn as_ref(&self) -> &ProfileError {
        self
    }
}
