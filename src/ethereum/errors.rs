use crate::errors::DatabaseError;
use super::contracts::ArtifactError;
use super::signatures::SignatureError;
use super::utils::AddressError;

#[derive(thiserror::Error, Debug)]
pub enum EthereumError {
    #[error("io error")]
    IoError(#[from] std::io::Error),

    #[error("json error")]
    JsonError(#[from] serde_json::Error),

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

    #[error("improprely configured")]
    ImproperlyConfigured,

    #[error("data conversion error")]
    ConversionError,

    #[error("token uri parsing error")]
    TokenUriParsingError,

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("signature error")]
    SigError(#[from] SignatureError),
}
