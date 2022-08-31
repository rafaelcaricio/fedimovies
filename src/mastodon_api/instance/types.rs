use serde::Serialize;
use serde_json::{to_value, Value};

use crate::config::{BlockchainConfig, Config};
use crate::ethereum::contracts::ContractSet;
use crate::mastodon_api::MASTODON_API_VERSION;

#[derive(Serialize)]
struct BlockchainFeatures {
    minter: bool,
    subscriptions: bool,
}

#[derive(Serialize)]
struct BlockchainInfo {
    chain_id: String,
    chain_metadata: Option<Value>,
    contract_address: Option<String>,
    features: BlockchainFeatures,
}

#[derive(Serialize)]
pub struct InstanceInfo {
    uri: String,
    title: String,
    short_description: String,
    description: String,
    version: String,
    registrations: bool,

    login_message: String,
    post_character_limit: usize,
    blockchains: Vec<BlockchainInfo>,
    ipfs_gateway_url: Option<String>,
}

fn get_full_api_version(version: &str) -> String {
    format!(
        "{0} (compatible; Mitra {1})",
        MASTODON_API_VERSION,
        version,
    )
}

impl InstanceInfo {
    pub fn create(config: &Config, maybe_blockchain: Option<&ContractSet>) -> Self {
        let mut blockchains = vec![];
        match config.blockchain() {
            Some(BlockchainConfig::Ethereum(ethereum_config)) => {
                let features = if let Some(contract_set) = maybe_blockchain {
                    BlockchainFeatures {
                        minter: contract_set.collectible.is_some(),
                        subscriptions: contract_set.subscription.is_some(),
                    }
                } else {
                    BlockchainFeatures {
                        minter: false,
                        subscriptions: false,
                    }
                };
                let maybe_chain_metadata = ethereum_config
                    .chain_metadata.as_ref()
                    .and_then(|metadata| to_value(metadata).ok());
                blockchains.push(BlockchainInfo {
                    chain_id: ethereum_config.chain_id.to_string(),
                    chain_metadata: maybe_chain_metadata,
                    contract_address:
                        Some(ethereum_config.contract_address.clone()),
                    features: features,
                });
            },
            Some(BlockchainConfig::Monero(monero_config)) => {
                let features = BlockchainFeatures {
                    minter: false,
                    subscriptions: true,
                };
                blockchains.push(BlockchainInfo {
                    chain_id: monero_config.chain_id.to_string(),
                    chain_metadata: None,
                    contract_address: None,
                    features: features,
                })
            },
            None => (),
        };
        Self {
            uri: config.instance().host(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: config.instance_description.clone(),
            version: get_full_api_version(&config.version),
            registrations: config.registrations_open,
            login_message: config.login_message.clone(),
            post_character_limit: config.post_character_limit,
            blockchains: blockchains,
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
