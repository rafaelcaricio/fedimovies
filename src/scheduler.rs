use std::collections::HashMap;
use std::time::Duration;

use anyhow::Error;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::config::{Config, Instance};
use crate::database::Pool;
use crate::ethereum::contracts::Blockchain;
use crate::ethereum::nft::process_nft_events;
use crate::ethereum::subscriptions::{
    check_ethereum_subscriptions,
    update_expired_subscriptions,
};

#[derive(Debug, Eq, Hash, PartialEq)]
enum Task {
    NftMonitor,
    EthereumSubscriptionMonitor,
    SubscriptionExpirationMonitor,
}

impl Task {
    /// Returns task period (in seconds)
    fn period(&self) -> i64 {
        match self {
            Self::NftMonitor => 30,
            Self::EthereumSubscriptionMonitor => 300,
            Self::SubscriptionExpirationMonitor => 300,
        }
    }
}

fn is_task_ready(last_run: &Option<DateTime<Utc>>, period: i64) -> bool {
    match last_run {
        Some(last_run) => {
            let time_passed = Utc::now() - *last_run;
            time_passed.num_seconds() >= period
        },
        None => true,
    }
}

async fn nft_monitor_task(
    maybe_blockchain: Option<&mut Blockchain>,
    db_pool: &Pool,
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

async fn ethereum_subscription_monitor_task(
    instance: &Instance,
    maybe_blockchain: Option<&mut Blockchain>,
    db_pool: &Pool,
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
        instance,
        &blockchain.contract_set.web3,
        subscription,
        &mut blockchain.sync_state,
        db_pool,
    ).await.map_err(Error::from)
}

pub fn run(
    config: Config,
    mut maybe_blockchain: Option<Blockchain>,
    db_pool: Pool,
) -> () {
    tokio::spawn(async move {
        let mut scheduler_state = HashMap::new();
        scheduler_state.insert(Task::NftMonitor, None);
        scheduler_state.insert(Task::EthereumSubscriptionMonitor, None);
        scheduler_state.insert(Task::SubscriptionExpirationMonitor, None);

        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut token_waitlist_map: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
        loop {
            interval.tick().await;

            for (task, last_run) in scheduler_state.iter_mut() {
                if !is_task_ready(last_run, task.period()) {
                    continue;
                };
                let task_result = match task {
                    Task::NftMonitor => {
                        nft_monitor_task(
                            maybe_blockchain.as_mut(),
                            &db_pool,
                            &mut token_waitlist_map,
                        ).await
                    },
                    Task::EthereumSubscriptionMonitor => {
                        ethereum_subscription_monitor_task(
                            &config.instance(),
                            maybe_blockchain.as_mut(),
                            &db_pool,
                        ).await
                    },
                    Task::SubscriptionExpirationMonitor => {
                        update_expired_subscriptions(
                            &config.instance(),
                            &db_pool,
                        ).await.map_err(Error::from)
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
