use serde::Serialize;
use serde_json::{to_value, Value};

use crate::config::{BlockchainConfig, Config};
use crate::ethereum::contracts::ContractSet;
use crate::mastodon_api::{
    MASTODON_API_VERSION,
    uploads::UPLOAD_MAX_SIZE,
};
use crate::models::posts::validators::ATTACHMENTS_MAX_NUM;
use crate::utils::{
    files::SUPPORTED_MEDIA_TYPES,
    markdown::markdown_to_html,
};

#[derive(Serialize)]
struct InstanceStats {
    user_count: i64,
    status_count: i64,
    domain_count: i64,
}

#[derive(Serialize)]
struct InstanceStatusLimits {
    max_characters: usize,
    max_media_attachments: usize,
}

#[derive(Serialize)]
struct InstanceMediaLimits {
    supported_mime_types: [&'static str; 4],
    image_size_limit: usize,
}

#[derive(Serialize)]
struct InstanceConfiguration {
    statuses: InstanceStatusLimits,
    media_attachments: InstanceMediaLimits,
}

#[derive(Serialize)]
struct BlockchainFeatures {
    gate: bool,
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

/// https://docs.joinmastodon.org/entities/V1_Instance/
#[derive(Serialize)]
pub struct InstanceInfo {
    uri: String,
    title: String,
    short_description: String,
    description: String,
    description_source: String,
    version: String,
    registrations: bool,
    stats: InstanceStats,
    configuration: InstanceConfiguration,

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
    pub fn create(
        config: &Config,
        maybe_blockchain: Option<&ContractSet>,
        user_count: i64,
        post_count: i64,
        peer_count: i64,
    ) -> Self {
        let mut blockchains = vec![];
        match config.blockchain() {
            Some(BlockchainConfig::Ethereum(ethereum_config)) => {
                let features = if let Some(contract_set) = maybe_blockchain {
                    BlockchainFeatures {
                        gate: contract_set.gate.is_some(),
                        minter: contract_set.collectible.is_some(),
                        subscriptions: contract_set.subscription.is_some(),
                    }
                } else {
                    BlockchainFeatures {
                        gate: false,
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
                    gate: false,
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
            uri: config.instance().hostname(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: markdown_to_html(&config.instance_description),
            description_source: config.instance_description.clone(),
            version: get_full_api_version(&config.version),
            registrations: config.registrations_open,
            stats: InstanceStats {
                user_count,
                status_count: post_count,
                domain_count: peer_count,
            },
            configuration: InstanceConfiguration {
                statuses: InstanceStatusLimits {
                    max_characters: config.post_character_limit,
                    max_media_attachments: ATTACHMENTS_MAX_NUM,
                },
                media_attachments: InstanceMediaLimits {
                    supported_mime_types: SUPPORTED_MEDIA_TYPES,
                    image_size_limit: UPLOAD_MAX_SIZE,
                },
            },
            login_message: config.login_message.clone(),
            post_character_limit: config.post_character_limit,
            blockchains: blockchains,
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
