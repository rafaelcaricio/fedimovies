use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::ethereum::utils::{parse_caip2_chain_id, ChainIdError};
use crate::utils::caip2::ChainId;

fn default_chain_sync_step() -> u64 { 1000 }

fn default_chain_reorg_max_depth() -> u64 { 10 }

#[derive(Clone, Deserialize)]
pub struct EthereumConfig {
    // CAIP-2 chain ID
    pub chain_id: ChainId,
    // Additional information for clients
    pub chain_info: Option<HashMap<String, String>>,
    pub contract_address: String,
    pub contract_dir: PathBuf,
    pub api_url: String,
    // Block explorer base URL (should be compatible with https://eips.ethereum.org/EIPS/eip-3091)
    pub explorer_url: Option<String>,
    // Instance private key
    pub signing_key: String,

    #[serde(default = "default_chain_sync_step")]
    pub chain_sync_step: u64,
    #[serde(default = "default_chain_reorg_max_depth")]
    pub chain_reorg_max_depth: u64,
}

impl EthereumConfig {
    pub fn try_ethereum_chain_id(&self) -> Result<u32, ChainIdError> {
        parse_caip2_chain_id(&self.chain_id)
    }

    pub fn ethereum_chain_id(&self) -> u32 {
        self.try_ethereum_chain_id().unwrap()
    }
}

#[derive(Clone, Deserialize)]
pub struct MoneroConfig {
    pub chain_id: ChainId,
    pub daemon_url: String,
    pub wallet_url: String,
    pub wallet_name: String,
    pub wallet_password: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum BlockchainConfig {
    Ethereum(EthereumConfig),
    Monero(MoneroConfig),
}

impl BlockchainConfig {
    pub fn ethereum_config(&self) -> Option<&EthereumConfig> {
        if let Self::Ethereum(ethereum_config) = self {
            Some(ethereum_config)
        } else {
            None
        }
    }

    pub fn monero_config(&self) -> Option<&MoneroConfig> {
        if let Self::Monero(monero_config) = self {
            Some(monero_config)
        } else {
            None
        }
    }
}
