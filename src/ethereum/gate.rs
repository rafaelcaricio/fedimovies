use web3::contract::{Contract, Options};

use crate::config::BlockchainConfig;
use super::api::connect;
use super::contracts::{MANAGER, load_abi};
use super::errors::EthereumError;
use super::utils::parse_address;

pub async fn is_allowed_user(
    config: &BlockchainConfig,
    user_address: &str,
) -> Result<bool, EthereumError> {
    let web3 = connect(&config.api_url)?;
    let manager_abi = load_abi(&config.contract_dir, MANAGER)?;
    let manager_address = parse_address(&config.contract_address)?;
    let manager = Contract::from_json(
        web3.eth(),
        manager_address,
        &manager_abi,
    )?;
    let user_address = parse_address(user_address)?;
    let result: bool = manager.query(
        "isAllowedUser", (user_address,),
        None, Options::default(), None,
    ).await?;
    Ok(result)
}
