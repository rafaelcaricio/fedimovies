use std::convert::TryInto;

use chrono::{DateTime, TimeZone, Utc};

use web3::{
    api::Web3,
    contract::Contract,
    ethabi::RawLog,
    transports::Http,
    types::{BlockId, BlockNumber, FilterBuilder, U256},
};

use crate::config::BlockchainConfig;
use crate::database::{Pool, get_database_client};
use crate::errors::{ConversionError, DatabaseError};
use crate::models::notifications::queries::create_subscription_notification;
use crate::models::profiles::currencies::Currency;
use crate::models::profiles::queries::search_profile_by_wallet_address;
use crate::models::relationships::queries::unsubscribe;
use crate::models::subscriptions::queries::{
    create_subscription,
    update_subscription,
    get_expired_subscriptions,
    get_subscription_by_participants,
};
use crate::models::users::queries::get_user_by_wallet_address;
use super::errors::EthereumError;
use super::signatures::{sign_contract_call, CallArgs, SignatureData};
use super::utils::{address_to_string, parse_address};

const ETHEREUM: Currency = Currency::Ethereum;

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
    from_block: u64,
    db_pool: &Pool,
) -> Result<(), EthereumError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let event_abi = contract.abi().event("UpdateSubscription")?;
    let filter = FilterBuilder::default()
        .address(vec![contract.address()])
        .topics(Some(vec![event_abi.signature()]), None, None, None)
        .from_block(BlockNumber::Number(from_block.into()))
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

        let profiles = search_profile_by_wallet_address(
            db_client,
            &ETHEREUM,
            &sender_address,
            true, // prefer verified addresses
        ).await?;
        let sender = match &profiles[..] {
            [profile] => profile,
            [] => {
                // Profile not found, skip event
                log::error!("unknown subscriber {}", sender_address);
                continue;
            },
            _ => {
                // Ambiguous results, skip event
                log::error!(
                    "search returned multiple results for address {}",
                    sender_address,
                );
                continue;
            },
        };
        let recipient = get_user_by_wallet_address(db_client, &recipient_address).await?;

        match get_subscription_by_participants(
            db_client,
            &sender.id,
            &recipient.id,
        ).await {
            Ok(subscription) => {
                if subscription.sender_address != sender_address {
                    // Trust only key/address that was linked to profile
                    // when first subscription event occured.
                    // Key rotation is not supported.
                    log::error!(
                        "subscriber address changed from {} to {}",
                        subscription.sender_address,
                        sender_address,
                    );
                    continue;
                };
                if subscription.updated_at < block_date {
                    // Update subscription expiration date
                    update_subscription(
                        db_client,
                        subscription.id,
                        &expires_at,
                        &block_date,
                    ).await?;
                    if expires_at > subscription.expires_at {
                        // Subscription was extended
                        create_subscription_notification(
                            db_client,
                            &subscription.sender_id,
                            &subscription.recipient_id,
                        ).await?;
                    };
                    log::info!(
                        "subscription updated: {0} to {1}",
                        subscription.sender_id,
                        subscription.recipient_id,
                    );
                };
            },
            Err(DatabaseError::NotFound(_)) => {
                // New subscription
                create_subscription(
                    db_client,
                    &sender.id,
                    &sender_address,
                    &recipient.id,
                    &expires_at,
                    &block_date,
                ).await?;
                create_subscription_notification(
                    db_client,
                    &sender.id,
                    &recipient.id,
                ).await?;
                log::info!(
                    "subscription created: {0} to {1}",
                    sender.id,
                    recipient.id,
                );
            },
            Err(other_error) => return Err(other_error.into()),
        };
    };

    for subscription in get_expired_subscriptions(db_client).await? {
        // Remove relationship
        unsubscribe(db_client, &subscription.sender_id, &subscription.recipient_id).await?;
        log::info!(
            "subscription expired: {0} to {1}",
            subscription.sender_id,
            subscription.recipient_id,
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
