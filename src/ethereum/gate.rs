use web3::contract::{Contract, Options};

use crate::config::Config;
use super::api::connect;
use super::nft::{MANAGER, load_abi, EthereumError};
use super::utils::parse_address;

pub async fn is_allowed_user(
    config: &Config,
    user_address: &str,
) -> Result<bool, EthereumError> {
    let json_rpc_url = config.ethereum_json_rpc_url.as_ref()
        .ok_or(EthereumError::ImproperlyConfigured)?;
    let web3 = connect(json_rpc_url)?;
    let ethereum_config = config.ethereum_contract.as_ref()
        .ok_or(EthereumError::ImproperlyConfigured)?;

    let manager_abi = load_abi(&config.contract_dir, MANAGER)?;
    let manager_address = parse_address(&ethereum_config.address)?;
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
