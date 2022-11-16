use web3::{
    contract::{Contract, Options},
    transports::Http,
};

use super::errors::EthereumError;
use super::utils::parse_address;

pub async fn is_allowed_user(
    gate: &Contract<Http>,
    user_address: &str,
) -> Result<bool, EthereumError> {
    let user_address = parse_address(user_address)?;
    let result: bool = gate.query(
        "isAllowedUser", (user_address,),
        None, Options::default(), None,
    ).await?;
    Ok(result)
}
