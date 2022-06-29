use std::fs;
use std::path::Path;

use web3::{
    api::Web3,
    contract::{Contract, Options},
    ethabi,
    transports::Http,
};

use crate::config::BlockchainConfig;
use super::api::connect;
use super::errors::EthereumError;
use super::sync::{
    get_current_block_number,
    SyncState,
};
use super::utils::parse_address;

const ADAPTER: &str = "IAdapter";
const SUBSCRIPTION: &str = "ISubscription";
const ERC721: &str = "IERC721Metadata";

#[derive(thiserror::Error, Debug)]
pub enum ArtifactError {
    #[error("io error")]
    IoError(#[from] std::io::Error),
    
    #[error("json error")]
    JsonError(#[from] serde_json::Error),

    #[error("key error")]
    KeyError,

    #[error(transparent)]
    AbiError(#[from] ethabi::Error),
}

fn load_abi(
    contract_dir: &Path,
    contract_name: &str,
) -> Result<ethabi::Contract, ArtifactError> {
    let artifact_path = contract_dir.join(format!("{}.json", contract_name));
    let artifact = fs::read_to_string(artifact_path)?;
    let artifact_value: serde_json::Value =
        serde_json::from_str(&artifact)?;
    let abi_json = artifact_value.get("abi")
        .ok_or(ArtifactError::KeyError)?
        .to_string();
    let abi = ethabi::Contract::load(abi_json.as_bytes())?;
    Ok(abi)
}

#[derive(Clone)]
pub struct ContractSet {
    pub web3: Web3<Http>,

    pub adapter: Contract<Http>,
    pub collectible: Contract<Http>,
    pub subscription: Contract<Http>,
}

#[derive(Clone)]
pub struct Blockchain {
    pub contract_set: ContractSet,
    pub sync_state: SyncState,
}

pub async fn get_contracts(
    config: &BlockchainConfig,
    storage_dir: &Path,
) -> Result<Blockchain, EthereumError> {
    let web3 = connect(&config.api_url)?;
    let chain_id = web3.eth().chain_id().await?;
    if chain_id != config.ethereum_chain_id().into() {
        return Err(EthereumError::ImproperlyConfigured("incorrect chain ID"));
    };
    let adapter_abi = load_abi(&config.contract_dir, ADAPTER)?;
    let adapter_address = parse_address(&config.contract_address)?;
    let adapter = Contract::new(
        web3.eth(),
        adapter_address,
        adapter_abi,
    );

    let collectible_address = adapter.query(
        "collectible",
        (), None, Options::default(), None,
    ).await?;
    let collectible_abi = load_abi(&config.contract_dir, ERC721)?;
    let collectible = Contract::new(
        web3.eth(),
        collectible_address,
        collectible_abi,
    );
    log::info!("collectible item contract address is {:?}", collectible.address());

    let subscription_address = adapter.query(
        "subscription",
        (), None, Options::default(), None,
    ).await?;
    let subscription_abi = load_abi(&config.contract_dir, SUBSCRIPTION)?;
    let subscription = Contract::new(
        web3.eth(),
        subscription_address,
        subscription_abi,
    );
    log::info!("subscription contract address is {:?}", subscription.address());

    let current_block = get_current_block_number(&web3, storage_dir).await?;
    log::info!("current block is {}", current_block);
    let sync_state = SyncState::new(
        current_block,
        vec![collectible.address(), subscription.address()],
        storage_dir,
    );

    let contract_set = ContractSet {
        web3,
        adapter,
        collectible,
        subscription,
    };
    Ok(Blockchain { contract_set, sync_state })
}
