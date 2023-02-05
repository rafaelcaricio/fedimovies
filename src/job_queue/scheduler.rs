use std::collections::HashMap;
use std::time::Duration;

use anyhow::Error;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::activitypub::queues::{
    process_queued_incoming_activities,
    process_queued_outgoing_activities,
};
use crate::config::{Config, Instance};
use crate::database::{get_database_client, DbPool};
use crate::ethereum::contracts::Blockchain;
use crate::ethereum::nft::process_nft_events;
use crate::ethereum::subscriptions::{
    check_ethereum_subscriptions,
    update_expired_subscriptions,
};
use crate::monero::subscriptions::check_monero_subscriptions;

#[derive(Debug, Eq, Hash, PartialEq)]
enum PeriodicTask {
    NftMonitor,
    EthereumSubscriptionMonitor,
    SubscriptionExpirationMonitor,
    MoneroPaymentMonitor,
    IncomingActivityQueueExecutor,
    OutgoingActivityQueueExecutor,
}

impl PeriodicTask {
    /// Returns task period (in seconds)
    fn period(&self) -> i64 {
        match self {
            Self::NftMonitor => 30,
            Self::EthereumSubscriptionMonitor => 300,
            Self::SubscriptionExpirationMonitor => 300,
            Self::MoneroPaymentMonitor => 30,
            Self::IncomingActivityQueueExecutor => 5,
            Self::OutgoingActivityQueueExecutor => 5,
        }
    }

    fn is_ready(&self, last_run: &Option<DateTime<Utc>>) -> bool {
        match last_run {
            Some(last_run) => {
                let time_passed = Utc::now() - *last_run;
                time_passed.num_seconds() >= self.period()
            },
            None => true,
        }
    }
}

async fn nft_monitor(
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

async fn ethereum_subscription_monitor(
    instance: &Instance,
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
        instance,
        &blockchain.contract_set.web3,
        subscription,
        &mut blockchain.sync_state,
        db_pool,
    ).await.map_err(Error::from)
}

async fn subscription_expiration_monitor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    update_expired_subscriptions(
        &config.instance(),
        db_pool,
    ).await?;
    Ok(())
}

async fn monero_payment_monitor(
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

async fn incoming_activity_queue_executor(
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

async fn outgoing_activity_queue_executor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    process_queued_outgoing_activities(config, db_pool).await?;
    Ok(())
}

pub fn run(
    config: Config,
    mut maybe_blockchain: Option<Blockchain>,
    db_pool: DbPool,
) -> () {
    tokio::spawn(async move {
        let mut scheduler_state = HashMap::from([
            (PeriodicTask::NftMonitor, None),
            (PeriodicTask::EthereumSubscriptionMonitor, None),
            (PeriodicTask::SubscriptionExpirationMonitor, None),
            (PeriodicTask::MoneroPaymentMonitor, None),
            (PeriodicTask::IncomingActivityQueueExecutor, None),
            (PeriodicTask::OutgoingActivityQueueExecutor, None),
        ]);

        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut token_waitlist_map: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
        loop {
            interval.tick().await;

            for (task, last_run) in scheduler_state.iter_mut() {
                if !task.is_ready(last_run) {
                    continue;
                };
                let task_result = match task {
                    PeriodicTask::NftMonitor => {
                        nft_monitor(
                            maybe_blockchain.as_mut(),
                            &db_pool,
                            &mut token_waitlist_map,
                        ).await
                    },
                    PeriodicTask::EthereumSubscriptionMonitor => {
                        ethereum_subscription_monitor(
                            &config.instance(),
                            maybe_blockchain.as_mut(),
                            &db_pool,
                        ).await
                    },
                    PeriodicTask::SubscriptionExpirationMonitor => {
                        subscription_expiration_monitor(&config, &db_pool).await
                    },
                    PeriodicTask::MoneroPaymentMonitor => {
                        monero_payment_monitor(&config, &db_pool).await
                    },
                    PeriodicTask::IncomingActivityQueueExecutor => {
                        incoming_activity_queue_executor(&config, &db_pool).await
                    },
                    PeriodicTask::OutgoingActivityQueueExecutor => {
                        outgoing_activity_queue_executor(&config, &db_pool).await
                    },
                };
                task_result.unwrap_or_else(|err| {
                    log::error!("{:?}: {}", task, err);
                });
                *last_run = Some(Utc::now());
            };
        };
    });
}
