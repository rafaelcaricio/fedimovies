use std::fs;
use std::path::Path;

use web3::{
    api::Web3,
    contract::{Contract, Error as ContractError, Options},
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

const ERC165: &str = "IERC165";
const GATE: &str = "IGate";
const MINTER: &str = "IMinter";
const SUBSCRIPTION_ADAPTER: &str = "ISubscriptionAdapter";
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

// https://eips.ethereum.org/EIPS/eip-165
// Interface identifier is the XOR of all function selectors in the interface
fn interface_signature(interface: &ethabi::Contract) -> [u8; 4] {
    interface.functions()
        .map(|func| func.short_signature())
        .fold([0; 4], |mut acc, item| {
            for i in 0..4 {
                acc[i] ^= item[i];
            };
            acc
        })
}

/// Returns true if contract supports interface (per ERC-165)
async fn is_interface_supported(
    contract: &Contract<Http>,
    interface: &ethabi::Contract,
) -> Result<bool, ContractError> {
    let signature = interface_signature(interface);
    contract.query(
        "supportsInterface",
        (signature,), None, Options::default(), None,
    ).await
}

#[derive(Clone)]
pub struct ContractSet {
    pub web3: Web3<Http>,

    pub gate: Option<Contract<Http>>,
    pub collectible: Option<Contract<Http>>,
    pub subscription: Option<Contract<Http>>,
    pub subscription_adapter: Option<Contract<Http>>,
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

    let adapter_address = parse_address(&config.contract_address)?;
    let erc165_abi = load_abi(&config.contract_dir, ERC165)?;
    let erc165 = Contract::new(
        web3.eth(),
        adapter_address,
        erc165_abi,
    );

    let mut maybe_gate = None;
    let mut maybe_collectible = None;
    let mut maybe_subscription = None;
    let mut maybe_subscription_adapter = None;
    let mut sync_targets = vec![];

    let gate_abi = load_abi(&config.contract_dir, GATE)?;
    if is_interface_supported(&erc165, &gate_abi).await? {
        let gate = Contract::new(
            web3.eth(),
            adapter_address,
            gate_abi,
        );
        maybe_gate = Some(gate);
        log::info!("found gate interface");
    };

    let minter_abi = load_abi(&config.contract_dir, MINTER)?;
    if is_interface_supported(&erc165, &minter_abi).await? {
        let minter = Contract::new(
            web3.eth(),
            adapter_address,
            minter_abi,
        );
        log::info!("found minter interface");
        let collectible_address = minter.query(
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
        sync_targets.push(collectible.address());
        maybe_collectible = Some(collectible);
    };

    let subscription_adapter_abi = load_abi(&config.contract_dir, SUBSCRIPTION_ADAPTER)?;
    if is_interface_supported(&erc165, &subscription_adapter_abi).await? {
        let subscription_adapter = Contract::new(
            web3.eth(),
            adapter_address,
            subscription_adapter_abi,
        );
        log::info!("found subscription interface");
        let subscription_address = subscription_adapter.query(
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
        sync_targets.push(subscription.address());
        maybe_subscription = Some(subscription);
        maybe_subscription_adapter = Some(subscription_adapter);
    };

    let current_block = get_current_block_number(&web3, storage_dir).await?;
    let sync_state = SyncState::new(
        current_block,
        sync_targets,
        config.chain_sync_step,
        config.chain_reorg_max_depth,
        storage_dir,
    );

    let contract_set = ContractSet {
        web3,
        gate: maybe_gate,
        collectible: maybe_collectible,
        subscription: maybe_subscription,
        subscription_adapter: maybe_subscription_adapter,
    };
    Ok(Blockchain { contract_set, sync_state })
}
