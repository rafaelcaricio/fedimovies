use crate::config::BlockchainConfig;
use super::errors::EthereumError;
use super::signatures::{sign_contract_call, CallArgs, SignatureData};
use super::utils::parse_address;

pub fn create_subscription_signature(
    blockchain_config: &BlockchainConfig,
    user_address: &str,
) -> Result<SignatureData, EthereumError> {
    let user_address = parse_address(user_address)?;
    let call_args: CallArgs = vec![Box::new(user_address)];
    let signature = sign_contract_call(
        &blockchain_config.signing_key,
        blockchain_config.ethereum_chain_id(),
        &blockchain_config.contract_address,
        "configureSubscription",
        call_args,
    )?;
    Ok(signature)
}
