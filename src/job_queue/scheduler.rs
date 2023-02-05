use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::config::Config;
use crate::database::DbPool;
use crate::ethereum::contracts::Blockchain;

use super::periodic_tasks::*;

#[derive(Debug, Eq, Hash, PartialEq)]
enum PeriodicTask {
    NftMonitor,
    EthereumSubscriptionMonitor,
    SubscriptionExpirationMonitor,
    MoneroPaymentMonitor,
    IncomingActivityQueueExecutor,
    OutgoingActivityQueueExecutor,
    DeleteExtraneousPosts,
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
            Self::DeleteExtraneousPosts => 3600,
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
        if config.retention.extraneous_posts.is_some() {
            scheduler_state.insert(PeriodicTask::DeleteExtraneousPosts, None);
        };

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
                            &config,
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
                    PeriodicTask::DeleteExtraneousPosts => {
                        delete_extraneous_posts(&config, &db_pool).await
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
