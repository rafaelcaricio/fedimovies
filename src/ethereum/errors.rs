use crate::errors::DatabaseError;
use super::contracts::ArtifactError;
use super::signatures::SignatureError;
use super::utils::AddressError;

#[derive(thiserror::Error, Debug)]
pub enum EthereumError {
    #[error("{0}")]
    ImproperlyConfigured(&'static str),

    #[error("invalid address")]
    InvalidAddress(#[from] AddressError),

    #[error(transparent)]
    Web3Error(#[from] web3::Error),

    #[error("artifact error")]
    ArtifactError(#[from] ArtifactError),

    #[error("abi error")]
    AbiError(#[from] web3::ethabi::Error),

    #[error("contract error")]
    ContractError(#[from] web3::contract::Error),

    #[error("data conversion error")]
    ConversionError,

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("signature error")]
    SignatureError(#[from] SignatureError),

    #[error("{0}")]
    OtherError(&'static str),
}
