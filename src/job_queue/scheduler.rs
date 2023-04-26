use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use fedimovies_config::Config;
use fedimovies_models::database::DbPool;

use super::periodic_tasks::*;

#[derive(Debug, Eq, Hash, PartialEq)]
enum PeriodicTask {
    SubscriptionExpirationMonitor,
    IncomingActivityQueueExecutor,
    OutgoingActivityQueueExecutor,
    DeleteExtraneousPosts,
    DeleteEmptyProfiles,
    PruneRemoteEmojis,
    HandleMoviesMentions,
}

impl PeriodicTask {
    /// Returns task period (in seconds)
    fn period(&self) -> i64 {
        match self {
            Self::SubscriptionExpirationMonitor => 300,
            Self::IncomingActivityQueueExecutor => 5,
            Self::OutgoingActivityQueueExecutor => 5,
            Self::DeleteExtraneousPosts => 3600,
            Self::DeleteEmptyProfiles => 3600,
            Self::PruneRemoteEmojis => 3600,
            Self::HandleMoviesMentions => 5,
        }
    }

    fn is_ready(&self, last_run: &Option<DateTime<Utc>>) -> bool {
        match last_run {
            Some(last_run) => {
                let time_passed = Utc::now() - *last_run;
                time_passed.num_seconds() >= self.period()
            }
            None => true,
        }
    }
}

pub fn run(config: Config, db_pool: DbPool) -> () {
    tokio::spawn(async move {
        let mut scheduler_state = HashMap::from([
            (PeriodicTask::SubscriptionExpirationMonitor, None),
            (PeriodicTask::IncomingActivityQueueExecutor, None),
            (PeriodicTask::OutgoingActivityQueueExecutor, None),
            (PeriodicTask::PruneRemoteEmojis, None),
            (PeriodicTask::HandleMoviesMentions, None),
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
                    PeriodicTask::IncomingActivityQueueExecutor => {
                        incoming_activity_queue_executor(&config, &db_pool).await
                    }
                    PeriodicTask::OutgoingActivityQueueExecutor => {
                        outgoing_activity_queue_executor(&config, &db_pool).await
                    }
                    PeriodicTask::DeleteExtraneousPosts => {
                        delete_extraneous_posts(&config, &db_pool).await
                    }
                    PeriodicTask::DeleteEmptyProfiles => {
                        delete_empty_profiles(&config, &db_pool).await
                    }
                    PeriodicTask::PruneRemoteEmojis => prune_remote_emojis(&config, &db_pool).await,
                    PeriodicTask::SubscriptionExpirationMonitor => Ok(()),
                    PeriodicTask::HandleMoviesMentions => {
                        handle_movies_mentions(&config, &db_pool).await
                    }
                };
                task_result.unwrap_or_else(|err| {
                    log::error!("{:?}: {}", task, err);
                });
                *last_run = Some(Utc::now());
            }
        }
    });
}
