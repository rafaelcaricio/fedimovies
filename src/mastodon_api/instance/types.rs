use serde::Serialize;

use crate::config::Config;
use crate::ethereum::nft::MINTER;

#[derive(Serialize)]
pub struct Instance {
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

impl From<&Config> for Instance {
    fn from(config: &Config) -> Self {
        Self {
            uri: config.instance_uri.clone(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: config.instance_description.clone(),
            version: config.version.clone(),
            registrations: config.registrations_open.clone(),
            login_message: config.login_message.clone(),
            ethereum_explorer_url: config.ethereum_explorer_url.clone(),
            nft_contract_name: config.ethereum_contract.as_ref()
                .and(Some(MINTER.into())),
            nft_contract_address: config.ethereum_contract.as_ref()
                .map(|val| val.address.clone()),
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
