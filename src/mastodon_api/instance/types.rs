use serde::Serialize;

use crate::config::Config;
use crate::ethereum::contracts::MANAGER;

#[derive(Serialize)]
pub struct InstanceInfo {
    uri: String,
    title: String,
    short_description: String,
    description: String,
    version: String,
    registrations: bool,

    login_message: String,
    ethereum_explorer_url: Option<String>,
    nft_contract_name: Option<String>,
    nft_contract_address: Option<String>,
    ipfs_gateway_url: Option<String>,
}

impl From<&Config> for InstanceInfo {
    fn from(config: &Config) -> Self {
        Self {
            uri: config.instance().host(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: config.instance_description.clone(),
            version: config.version.clone(),
            registrations: config.registrations_open,
            login_message: config.login_message.clone(),
            ethereum_explorer_url: config.blockchain.as_ref()
                .and_then(|val| val.explorer_url.clone()),
            nft_contract_name: config.blockchain.as_ref()
                .and(Some(MANAGER.into())),
            nft_contract_address: config.blockchain.as_ref()
                .map(|val| val.contract_address.clone()),
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
