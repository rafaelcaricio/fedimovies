use std::fs;
use std::path::Path;

use web3::{
    api::Web3,
    contract::{Contract, Options},
    transports::Http,
};

use crate::config::BlockchainConfig;
use super::api::connect;
use super::errors::EthereumError;
use super::utils::parse_address;

pub const ADAPTER: &str = "IAdapter";
pub const SUBSCRIPTION: &str = "ISubscription";
pub const ERC20: &str = "IERC20";
pub const ERC721: &str = "IERC721Metadata";

#[derive(thiserror::Error, Debug)]
pub enum ArtifactError {
    #[error("io error")]
    IoError(#[from] std::io::Error),
    
    #[error("json error")]
    JsonError(#[from] serde_json::Error),

    #[error("key error")]
    KeyError,
}

pub fn load_abi(
    contract_dir: &Path,
    contract_name: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let contract_artifact_path = contract_dir.join(format!("{}.json", contract_name));
    let contract_artifact = fs::read_to_string(contract_artifact_path)?;
    let contract_artifact_value: serde_json::Value =
        serde_json::from_str(&contract_artifact)?;
    let contract_abi = contract_artifact_value.get("abi")
        .ok_or(ArtifactError::KeyError)?
        .to_string().as_bytes().to_vec();
    Ok(contract_abi)
}

pub struct ContractSet {
    pub web3: Web3<Http>,

    #[allow(dead_code)]
    pub adapter: Contract<Http>,

    pub collectible: Contract<Http>,
    pub subscription: Contract<Http>,
}

pub async fn get_contracts(
    config: &BlockchainConfig,
) -> Result<ContractSet, EthereumError> {
    let web3 = connect(&config.api_url)?;
    let adapter_abi = load_abi(&config.contract_dir, ADAPTER)?;
    let adapter_address = parse_address(&config.contract_address)?;
    let adapter = Contract::from_json(
        web3.eth(),
        adapter_address,
        &adapter_abi,
    )?;

    let collectible_address = adapter.query(
        "collectible",
        (), None, Options::default(), None,
    ).await?;
    let collectibe_abi = load_abi(&config.contract_dir, ERC721)?;
    let collectible = Contract::from_json(
        web3.eth(),
        collectible_address,
        &collectibe_abi,
    )?;
    log::info!("collectible item contract address is {:?}", collectible.address());

    let subscription_address = adapter.query(
        "subscription",
        (), None, Options::default(), None,
    ).await?;
    let subscription_abi = load_abi(&config.contract_dir, SUBSCRIPTION)?;
    let subscription = Contract::from_json(
        web3.eth(),
        subscription_address,
        &subscription_abi,
    )?;
    log::info!("subscription contract address is {:?}", subscription.address());

    let contract_set = ContractSet {
        web3,
        adapter,
        collectible,
        subscription,
    };
    Ok(contract_set)
}
