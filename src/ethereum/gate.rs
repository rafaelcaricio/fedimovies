use web3::contract::Options;

use super::contracts::ContractSet;
use super::errors::EthereumError;
use super::utils::parse_address;

pub async fn is_allowed_user(
    contract_set: &ContractSet,
    user_address: &str,
) -> Result<bool, EthereumError> {
    let user_address = parse_address(user_address)?;
    let result: bool = contract_set.adapter.query(
        "isAllowedUser", (user_address,),
        None, Options::default(), None,
    ).await?;
    Ok(result)
}
