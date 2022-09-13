use std::convert::TryInto;

use chrono::{DateTime, TimeZone, Utc};
use tokio_postgres::GenericClient;

use web3::{
    api::Web3,
    contract::{Contract, Options},
    ethabi::RawLog,
    transports::Http,
    types::{BlockId, BlockNumber, FilterBuilder, U256},
};

use crate::activitypub::builders::{
    add_person::prepare_add_person,
    remove_person::prepare_remove_person,
};
use crate::activitypub::identifiers::LocalActorCollection;
use crate::config::{EthereumConfig, Instance};
use crate::database::{Pool, get_database_client};
use crate::errors::{ConversionError, DatabaseError};
use crate::models::notifications::queries::{
    create_subscription_notification,
    create_subscription_expiration_notification,
};
use crate::models::profiles::queries::{
    get_profile_by_id,
    search_profile_by_wallet_address,
};
use crate::models::profiles::types::DbActorProfile;
use crate::models::relationships::queries::unsubscribe;
use crate::models::subscriptions::queries::{
    create_subscription,
    update_subscription,
    get_expired_subscriptions,
    get_subscription_by_participants,
};
use crate::models::users::queries::{
    get_user_by_id,
    get_user_by_wallet_address,
};
use crate::models::users::types::User;
use crate::utils::caip2::ChainId;
use crate::utils::currencies::Currency;
use super::contracts::ContractSet;
use super::errors::EthereumError;
use super::signatures::{
    encode_uint256,
    sign_contract_call,
    CallArgs,
    SignatureData,
};
use super::sync::SyncState;
use super::utils::{address_to_string, parse_address};

const ETHEREUM: Currency = Currency::Ethereum;

fn u256_to_date(value: U256) -> Result<DateTime<Utc>, ConversionError> {
    let timestamp: i64 = value.try_into().map_err(|_| ConversionError)?;
    let datetime = Utc.timestamp_opt(timestamp, 0)
        .single()
        .ok_or(ConversionError)?;
    Ok(datetime)
}

pub async fn send_subscription_notifications(
    db_client: &impl GenericClient,
    instance: &Instance,
    sender: &DbActorProfile,
    recipient: &User,
) -> Result<(), DatabaseError> {
    create_subscription_notification(
        db_client,
        &sender.id,
        &recipient.id,
    ).await?;
    if let Some(ref remote_sender) = sender.actor_json {
        prepare_add_person(
            instance,
            recipient,
            remote_sender,
            LocalActorCollection::Subscribers,
        ).spawn_deliver();
    };
    Ok(())
}

/// Search for subscription update events
pub async fn check_ethereum_subscriptions(
    config: &EthereumConfig,
    instance: &Instance,
    web3: &Web3<Http>,
    contract: &Contract<Http>,
    sync_state: &mut SyncState,
    db_pool: &Pool,
) -> Result<(), EthereumError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let event_abi = contract.abi().event("UpdateSubscription")?;
    let (from_block, to_block) = sync_state.get_scan_range(
        &contract.address(),
        web3.eth().block_number().await?.as_u64(),
    );
    let filter = FilterBuilder::default()
        .address(vec![contract.address()])
        .topics(Some(vec![event_abi.signature()]), None, None, None)
        .from_block(BlockNumber::Number(from_block.into()))
        .to_block(BlockNumber::Number(to_block.into()))
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
        let recipient = get_user_by_wallet_address(
            db_client,
            &ETHEREUM,
            &recipient_address,
        ).await?;

        match get_subscription_by_participants(
            db_client,
            &sender.id,
            &recipient.id,
        ).await {
            Ok(subscription) => {
                let current_sender_address =
                    subscription.sender_address.unwrap_or("''".to_string());
                if current_sender_address != sender_address {
                    // Trust only key/address that was linked to profile
                    // when first subscription event occured.
                    // Key rotation is not supported.
                    log::error!(
                        "subscriber address changed from {} to {}",
                        current_sender_address,
                        sender_address,
                    );
                    continue;
                };
                if subscription.chain_id != config.chain_id &&
                    subscription.chain_id != ChainId::ethereum_devnet()
                {
                    // Switching from from devnet is allowed during migration
                    // because there's no persistent state
                    log::error!("can't switch to another chain");
                    continue;
                };
                if subscription.updated_at >= block_date {
                    // Event already processed
                    continue;
                };
                // Update subscription expiration date
                // TODO: disallow automatic chain ID updates after migration
                update_subscription(
                    db_client,
                    subscription.id,
                    &config.chain_id,
                    &expires_at,
                    &block_date,
                ).await?;
                #[allow(clippy::comparison_chain)]
                if expires_at > subscription.expires_at {
                    log::info!(
                        "subscription extended: {0} to {1}",
                        subscription.sender_id,
                        subscription.recipient_id,
                    );
                    send_subscription_notifications(
                        db_client,
                        instance,
                        sender,
                        &recipient,
                    ).await?;
                } else if expires_at < subscription.expires_at {
                    log::info!(
                        "subscription cancelled: {0} to {1}",
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
                    Some(&sender_address),
                    &recipient.id,
                    &config.chain_id,
                    &expires_at,
                    &block_date,
                ).await?;
                log::info!(
                    "subscription created: {0} to {1}",
                    sender.id,
                    recipient.id,
                );
                send_subscription_notifications(
                    db_client,
                    instance,
                    sender,
                    &recipient,
                ).await?;
            },
            Err(other_error) => return Err(other_error.into()),
        };
    };

    sync_state.update(&contract.address(), to_block)?;
    Ok(())
}

pub async fn update_expired_subscriptions(
    instance: &Instance,
    db_pool: &Pool,
) -> Result<(), EthereumError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    for subscription in get_expired_subscriptions(db_client).await? {
        // Remove relationship
        unsubscribe(db_client, &subscription.sender_id, &subscription.recipient_id).await?;
        log::info!(
            "subscription expired: {0} to {1}",
            subscription.sender_id,
            subscription.recipient_id,
        );
        let sender = get_profile_by_id(db_client, &subscription.sender_id).await?;
        if let Some(ref remote_sender) = sender.actor_json {
            let recipient = get_user_by_id(db_client, &subscription.recipient_id).await?;
            prepare_remove_person(
                instance,
                &recipient,
                remote_sender,
                LocalActorCollection::Subscribers,
            ).spawn_deliver();
        } else {
            create_subscription_expiration_notification(
                db_client,
                &subscription.recipient_id,
                &subscription.sender_id,
            ).await?;
        };
    };
    Ok(())
}

pub fn create_subscription_signature(
    blockchain_config: &EthereumConfig,
    user_address: &str,
    price: u64,
) -> Result<SignatureData, EthereumError> {
    let user_address = parse_address(user_address)?;
    let call_args: CallArgs = vec![
        Box::new(user_address),
        Box::new(encode_uint256(price)),
    ];
    let signature = sign_contract_call(
        &blockchain_config.signing_key,
        blockchain_config.ethereum_chain_id(),
        &blockchain_config.contract_address,
        "configureSubscription",
        call_args,
    )?;
    Ok(signature)
}

pub async fn is_registered_recipient(
    contract_set: &ContractSet,
    user_address: &str,
) -> Result<bool, EthereumError> {
    let adapter = match &contract_set.subscription_adapter {
        Some(contract) => contract,
        None => return Ok(false),
    };
    let user_address = parse_address(user_address)?;
    let result: bool = adapter.query(
        "isSubscriptionConfigured", (user_address,),
        None, Options::default(), None,
    ).await?;
    Ok(result)
}
