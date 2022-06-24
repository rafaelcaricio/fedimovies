use std::collections::HashMap;
use std::time::Duration;

use anyhow::Error;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::config::Config;
use crate::database::Pool;
use crate::ethereum::contracts::Blockchain;
use crate::ethereum::nft::process_nft_events;
use crate::ethereum::subscriptions::check_subscriptions;

#[derive(Debug, Eq, Hash, PartialEq)]
enum Task {
    NftMonitor,
    SubscriptionMonitor,
}

impl Task {
    /// Returns task period (in seconds)
    fn period(&self) -> i64 {
        match self {
            Self::NftMonitor => 30,
            Self::SubscriptionMonitor => 300,
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

pub fn run(
    _config: Config,
    mut maybe_blockchain: Option<Blockchain>,
    db_pool: Pool,
) -> () {
    tokio::spawn(async move {
        let mut scheduler_state = HashMap::new();
        scheduler_state.insert(Task::NftMonitor, None);
        scheduler_state.insert(Task::SubscriptionMonitor, None);

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
                        if let Some(blockchain) = maybe_blockchain.as_mut() {
                            // Monitor events only if ethereum integration is enabled
                            process_nft_events(
                                &blockchain.contract_set.web3,
                                &blockchain.contract_set.collectible,
                                &mut blockchain.sync_state,
                                &db_pool,
                                &mut token_waitlist_map,
                            ).await.map_err(Error::from)
                        } else { Ok(()) }
                    },
                    Task::SubscriptionMonitor => {
                        if let Some(blockchain) = maybe_blockchain.as_mut() {
                            check_subscriptions(
                                &blockchain.contract_set.web3,
                                &blockchain.contract_set.subscription,
                                &mut blockchain.sync_state,
                                &db_pool,
                            ).await.map_err(Error::from)
                        } else { Ok(()) }
                    },
                };
                task_result.unwrap_or_else(|err| {
                    log::error!("{:?}: {}", task, err);
                });
                *last_run = Some(Utc::now());
            };
        }
    });
}
