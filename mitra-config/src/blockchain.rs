use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use mitra_utils::caip2::{ChainId, ChainIdError};

fn default_chain_sync_step() -> u64 { 1000 }

fn default_chain_reorg_max_depth() -> u64 { 10 }

#[derive(Clone, Deserialize, Serialize)]
pub struct EthereumChainMetadata {
    pub chain_name: String,
    pub currency_name: String,
    pub currency_symbol: String,
    pub currency_decimals: u8,
    pub public_api_url: String,
    // Block explorer base URL (should be compatible with https://eips.ethereum.org/EIPS/eip-3091)
    pub explorer_url: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct EthereumConfig {
    // CAIP-2 chain ID
    pub chain_id: ChainId,
    // Additional information for clients
    // https://github.com/ethereum-lists/chains
    pub chain_metadata: Option<EthereumChainMetadata>,

    pub contract_address: String,
    pub contract_dir: PathBuf,
    pub api_url: String,
    // Instance private key
    pub signing_key: String,

    #[serde(default = "default_chain_sync_step")]
    pub chain_sync_step: u64,
    #[serde(default = "default_chain_reorg_max_depth")]
    pub chain_reorg_max_depth: u64,
}

impl EthereumConfig {
    pub fn try_ethereum_chain_id(&self) -> Result<u32, ChainIdError> {
        self.chain_id.ethereum_chain_id()
    }

    pub fn ethereum_chain_id(&self) -> u32 {
        self.try_ethereum_chain_id().unwrap()
    }
}

fn default_wallet_account_index() -> u32 { 0 }

#[derive(Clone, Deserialize)]
pub struct MoneroConfig {
    pub chain_id: ChainId,
    #[serde(alias = "daemon_url")]
    pub node_url: String,
    #[serde(alias = "wallet_url")]
    pub wallet_rpc_url: String,
    pub wallet_rpc_username: Option<String>,
    pub wallet_rpc_password: Option<String>,
    // Wallet name and password are required when
    // monero-wallet-rpc is running with --wallet-dir option
    pub wallet_name: Option<String>,
    pub wallet_password: Option<String>,
    #[serde(default = "default_wallet_account_index")]
    pub account_index: u32,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum BlockchainConfig {
    Ethereum(EthereumConfig),
    Monero(MoneroConfig),
}
