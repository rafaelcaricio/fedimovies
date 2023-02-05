use std::collections::HashMap;
use std::time::Duration;

use anyhow::Error;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::activitypub::queues::{
    process_queued_incoming_activities,
    process_queued_outgoing_activities,
};
use crate::config::Config;
use crate::database::{get_database_client, DbPool};
use crate::ethereum::contracts::Blockchain;
use crate::ethereum::nft::process_nft_events;
use crate::ethereum::subscriptions::{
    check_ethereum_subscriptions,
    update_expired_subscriptions,
};
use crate::monero::subscriptions::check_monero_subscriptions;

pub async fn nft_monitor(
    maybe_blockchain: Option<&mut Blockchain>,
    db_pool: &DbPool,
    token_waitlist_map: &mut HashMap<Uuid, DateTime<Utc>>,
) -> Result<(), Error> {
    let blockchain = match maybe_blockchain {
        Some(blockchain) => blockchain,
        None => return Ok(()),
    };
    let collectible = match &blockchain.contract_set.collectible {
        Some(contract) => contract,
        None => return Ok(()), // feature not enabled
    };
    process_nft_events(
        &blockchain.contract_set.web3,
        collectible,
        &mut blockchain.sync_state,
        db_pool,
        token_waitlist_map,
    ).await?;
    Ok(())
}

pub async fn ethereum_subscription_monitor(
    config: &Config,
    maybe_blockchain: Option<&mut Blockchain>,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let blockchain = match maybe_blockchain {
        Some(blockchain) => blockchain,
        None => return Ok(()),
    };
    let subscription = match &blockchain.contract_set.subscription {
        Some(contract) => contract,
        None => return Ok(()), // feature not enabled
    };
    check_ethereum_subscriptions(
        &blockchain.config,
        &config.instance(),
        &blockchain.contract_set.web3,
        subscription,
        &mut blockchain.sync_state,
        db_pool,
    ).await.map_err(Error::from)
}

pub async fn subscription_expiration_monitor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    update_expired_subscriptions(
        &config.instance(),
        db_pool,
    ).await?;
    Ok(())
}

pub async fn monero_payment_monitor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let maybe_monero_config = config.blockchain()
        .and_then(|conf| conf.monero_config());
    let monero_config = match maybe_monero_config {
        Some(monero_config) => monero_config,
        None => return Ok(()), // not configured
    };
    check_monero_subscriptions(
        &config.instance(),
        monero_config,
        db_pool,
    ).await?;
    Ok(())
}

pub async fn incoming_activity_queue_executor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let duration_max = Duration::from_secs(600);
    let completed = process_queued_incoming_activities(config, db_client);
    match tokio::time::timeout(duration_max, completed).await {
        Ok(result) => result?,
        Err(_) => log::error!("incoming activity queue executor timeout"),
    };
    Ok(())
}

pub async fn outgoing_activity_queue_executor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    process_queued_outgoing_activities(config, db_pool).await?;
    Ok(())
}
