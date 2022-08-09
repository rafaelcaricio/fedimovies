use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::ethereum::utils::{parse_caip2_chain_id, ChainIdError};

fn default_chain_sync_step() -> u64 { 1000 }

fn default_chain_reorg_max_depth() -> u64 { 10 }

#[derive(Clone, Deserialize)]
pub struct BlockchainConfig {
    // CAIP-2 chain ID (https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md)
    pub chain_id: String,
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

impl BlockchainConfig {
    pub fn try_ethereum_chain_id(&self) -> Result<u32, ChainIdError> {
        parse_caip2_chain_id(&self.chain_id)
    }

    pub fn ethereum_chain_id(&self) -> u32 {
        self.try_ethereum_chain_id().unwrap()
    }
}
