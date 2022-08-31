use serde::Serialize;
use serde_json::{to_value, Value};

use crate::config::Config;
use crate::ethereum::contracts::ContractSet;
use crate::mastodon_api::MASTODON_API_VERSION;

#[derive(Serialize)]
struct BlockchainFeatures {
    minter: bool,
    subscription: bool,
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
    blockchain_id: Option<String>,
    blockchain_explorer_url: Option<String>,
    blockchain_contract_address: Option<String>,
    blockchain_features: Option<BlockchainFeatures>,
    blockchain_info: Option<Value>,
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
        let ethereum_config = config.blockchain()
            .and_then(|conf| conf.ethereum_config());
        let blockchain_features = maybe_blockchain.map(|contract_set| {
            BlockchainFeatures {
                minter: contract_set.collectible.is_some(),
                subscription: contract_set.subscription.is_some(),
            }
        });
        let maybe_blockchain_info = ethereum_config
            .and_then(|conf| conf.chain_metadata.as_ref())
            .and_then(|metadata| to_value(metadata).ok());
        Self {
            uri: config.instance().host(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: config.instance_description.clone(),
            version: get_full_api_version(&config.version),
            registrations: config.registrations_open,
            login_message: config.login_message.clone(),
            post_character_limit: config.post_character_limit,
            blockchain_id: ethereum_config
                .map(|val| val.chain_id.to_string()),
            blockchain_explorer_url: ethereum_config
                .and_then(|conf| conf.chain_metadata.as_ref())
                .and_then(|metadata| metadata.explorer_url.clone()),
            blockchain_contract_address: ethereum_config
                .map(|val| val.contract_address.clone()),
            blockchain_features: blockchain_features,
            blockchain_info: maybe_blockchain_info,
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
