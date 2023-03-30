use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use mitra_config::Config;
use mitra_models::database::DbPool;

use crate::ethereum::contracts::Blockchain;
use super::periodic_tasks::*;

#[derive(Debug, Eq, Hash, PartialEq)]
enum PeriodicTask {
    EthereumSubscriptionMonitor,
    SubscriptionExpirationMonitor,
    MoneroPaymentMonitor,
    IncomingActivityQueueExecutor,
    OutgoingActivityQueueExecutor,
    DeleteExtraneousPosts,
    DeleteEmptyProfiles,
    PruneRemoteEmojis,

    #[cfg(feature = "ethereum-extras")]
    NftMonitor,
}

impl PeriodicTask {
    /// Returns task period (in seconds)
    fn period(&self) -> i64 {
        match self {
            Self::EthereumSubscriptionMonitor => 300,
            Self::SubscriptionExpirationMonitor => 300,
            Self::MoneroPaymentMonitor => 30,
            Self::IncomingActivityQueueExecutor => 5,
            Self::OutgoingActivityQueueExecutor => 5,
            Self::DeleteExtraneousPosts => 3600,
            Self::DeleteEmptyProfiles => 3600,
            Self::PruneRemoteEmojis => 3600,

            #[cfg(feature = "ethereum-extras")]
            Self::NftMonitor => 30,
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
            (PeriodicTask::EthereumSubscriptionMonitor, None),
            (PeriodicTask::SubscriptionExpirationMonitor, None),
            (PeriodicTask::MoneroPaymentMonitor, None),
            (PeriodicTask::IncomingActivityQueueExecutor, None),
            (PeriodicTask::OutgoingActivityQueueExecutor, None),
            (PeriodicTask::PruneRemoteEmojis, None),

            #[cfg(feature = "ethereum-extras")]
            (PeriodicTask::NftMonitor, None),
        ]);
        if config.retention.extraneous_posts.is_some() {
            scheduler_state.insert(PeriodicTask::DeleteExtraneousPosts, None);
        };
        if config.retention.empty_profiles.is_some() {
            scheduler_state.insert(PeriodicTask::DeleteEmptyProfiles, None);
        };

        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;

            for (task, last_run) in scheduler_state.iter_mut() {
                if !task.is_ready(last_run) {
                    continue;
                };
                let task_result = match task {
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
                    PeriodicTask::DeleteEmptyProfiles => {
                        delete_empty_profiles(&config, &db_pool).await
                    },
                    PeriodicTask::PruneRemoteEmojis => {
                        prune_remote_emojis(&config, &db_pool).await
                    },
                    #[cfg(feature = "ethereum-extras")]
                    PeriodicTask::NftMonitor => {
                        nft_monitor(
                            maybe_blockchain.as_mut(),
                            &db_pool,
                        ).await
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
