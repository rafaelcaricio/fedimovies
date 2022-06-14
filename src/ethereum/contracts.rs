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
use super::sync::{
    get_current_block_number,
    CHAIN_REORG_MAX_DEPTH,
};
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
    // Last synced block
    pub current_block: u64,

    #[allow(dead_code)]
    pub adapter: Contract<Http>,

    pub collectible: Contract<Http>,
    pub subscription: Contract<Http>,
}

pub async fn get_contracts(
    config: &BlockchainConfig,
    storage_dir: &Path,
) -> Result<ContractSet, EthereumError> {
    let web3 = connect(&config.api_url)?;
    let chain_id = web3.eth().chain_id().await?;
    if chain_id != config.ethereum_chain_id().into() {
        return Err(EthereumError::ImproperlyConfigured("incorrect chain ID"));
    };
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
    let collectible_abi = load_abi(&config.contract_dir, ERC721)?;
    let collectible = Contract::from_json(
        web3.eth(),
        collectible_address,
        &collectible_abi,
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

    let current_block = get_current_block_number(&web3, storage_dir).await?;
    log::info!("current block is {}", current_block);
    // Take reorgs into account
    let current_block = current_block.saturating_sub(CHAIN_REORG_MAX_DEPTH);

    let contract_set = ContractSet {
        web3,
        current_block,
        adapter,
        collectible,
        subscription,
    };
    Ok(contract_set)
}
