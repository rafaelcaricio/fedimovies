use std::time::Duration;

use anyhow::Error;

use mitra_config::Config;
use mitra_utils::datetime::days_before_now;

use crate::activitypub::queues::{
    process_queued_incoming_activities,
    process_queued_outgoing_activities,
};
use crate::database::{get_database_client, DbPool};
use crate::ethereum::{
    contracts::Blockchain,
    subscriptions::{
        check_ethereum_subscriptions,
        update_expired_subscriptions,
    },
};
use crate::monero::subscriptions::check_monero_subscriptions;
use crate::models::{
    posts::queries::{delete_post, find_extraneous_posts},
    profiles::queries::{
        delete_profile,
        find_empty_profiles,
        get_profile_by_id,
    },
};

#[cfg(feature = "ethereum-extras")]
use crate::ethereum::nft::process_nft_events;

#[cfg(feature = "ethereum-extras")]
pub async fn nft_monitor(
    maybe_blockchain: Option<&mut Blockchain>,
    db_pool: &DbPool,
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

pub async fn delete_extraneous_posts(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let updated_before = match config.retention.extraneous_posts {
        Some(days) => days_before_now(days),
        None => return Ok(()), // not configured
    };
    let posts = find_extraneous_posts(db_client, &updated_before).await?;
    for post_id in posts {
        let deletion_queue = delete_post(db_client, &post_id).await?;
        deletion_queue.process(config).await;
        log::info!("deleted post {}", post_id);
    };
    Ok(())
}

pub async fn delete_empty_profiles(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let updated_before = match config.retention.empty_profiles {
        Some(days) => days_before_now(days),
        None => return Ok(()), // not configured
    };
    let profiles = find_empty_profiles(db_client, &updated_before).await?;
    for profile_id in profiles {
        let profile = get_profile_by_id(db_client, &profile_id).await?;
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        deletion_queue.process(config).await;
        log::info!("deleted profile {}", profile.acct);
    };
    Ok(())
}
