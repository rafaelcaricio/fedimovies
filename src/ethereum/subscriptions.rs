use std::convert::TryInto;

use chrono::{DateTime, TimeZone, Utc};

use web3::{
    api::Web3,
    contract::Contract,
    ethabi::RawLog,
    transports::Http,
    types::{Address, BlockId, BlockNumber, FilterBuilder, U256},
};

use crate::config::BlockchainConfig;
use crate::database::{Pool, get_database_client};
use crate::errors::ConversionError;
use super::errors::EthereumError;
use super::signatures::{sign_contract_call, CallArgs, SignatureData};
use super::utils::parse_address;

/// Converts address object to lowercase hex string
fn address_to_string(address: Address) -> String {
    format!("{:#x}", address)
}

fn u256_to_date(value: U256) -> Result<DateTime<Utc>, ConversionError> {
    let timestamp: i64 = value.try_into().map_err(|_| ConversionError)?;
    let datetime = Utc.timestamp_opt(timestamp, 0)
        .single()
        .ok_or(ConversionError)?;
    Ok(datetime)
}

/// Search for subscription update events
pub async fn check_subscriptions(
    web3: &Web3<Http>,
    contract: &Contract<Http>,
    db_pool: &Pool,
) -> Result<(), EthereumError> {
    let _db_client = &**get_database_client(db_pool).await?;
    let event_abi = contract.abi().event("UpdateSubscription")?;
    let filter = FilterBuilder::default()
        .address(vec![contract.address()])
        .topics(Some(vec![event_abi.signature()]), None, None, None)
        .from_block(BlockNumber::Earliest)
        .build();
    let logs = web3.eth().logs(filter).await?;
    for log in logs {
        let block_number = if let Some(block_number) = log.block_number {
            block_number
        } else {
            // Skips logs without block number
            continue;
        };
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.clone().0,
        };
        let event = event_abi.parse_log(raw_log)?;
        let sender_address = event.params[0].value.clone().into_address()
            .map(address_to_string)
            .ok_or(EthereumError::ConversionError)?;
        let recipient_address = event.params[1].value.clone().into_address()
            .map(address_to_string)
            .ok_or(EthereumError::ConversionError)?;
        let expires_at_timestamp = event.params[2].value.clone().into_uint()
            .ok_or(EthereumError::ConversionError)?;
        let expires_at = u256_to_date(expires_at_timestamp)
            .map_err(|_| EthereumError::ConversionError)?;
        let block_id = BlockId::Number(BlockNumber::Number(block_number));
        let block_timestamp = web3.eth().block(block_id).await?
            .ok_or(EthereumError::ConversionError)?
            .timestamp;
        let block_date = u256_to_date(block_timestamp)
            .map_err(|_| EthereumError::ConversionError)?;
        log::info!(
            "subscription: from {0} to {1}, expires at {2}, updated at {3}",
            sender_address,
            recipient_address,
            expires_at,
            block_date,
        );
    };
    Ok(())
}

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
