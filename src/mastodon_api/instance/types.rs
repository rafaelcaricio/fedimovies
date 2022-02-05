use serde::Serialize;

use crate::config::Config;
use crate::mastodon_api::MASTODON_API_VERSION;

#[derive(Serialize)]
pub struct InstanceInfo {
    uri: String,
    title: String,
    short_description: String,
    description: String,
    version: String,
    registrations: bool,

    login_message: String,
    blockchain_explorer_url: Option<String>,
    blockchain_contract_address: Option<String>,
    ipfs_gateway_url: Option<String>,
}

fn get_full_api_version(version: &str) -> String {
    format!(
        "{0} (compatible; Mitra {1})",
        MASTODON_API_VERSION,
        version,
    )
}

impl From<&Config> for InstanceInfo {
    fn from(config: &Config) -> Self {
        Self {
            uri: config.instance().host(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: config.instance_description.clone(),
            version: get_full_api_version(&config.version),
            registrations: config.registrations_open,
            login_message: config.login_message.clone(),
            blockchain_explorer_url: config.blockchain.as_ref()
                .and_then(|val| val.explorer_url.clone()),
            blockchain_contract_address: config.blockchain.as_ref()
                .map(|val| val.contract_address.clone()),
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
