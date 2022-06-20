use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::config::Config;
use crate::database::Pool;
use crate::ethereum::contracts::ContractSet;
use crate::ethereum::nft::process_nft_events;
use crate::ethereum::subscriptions::check_subscriptions;

#[derive(Eq, Hash, PartialEq)]
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
    maybe_contract_set: Option<ContractSet>,
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
                match task {
                    Task::NftMonitor => {
                        if let Some(contract_set) = maybe_contract_set.as_ref() {
                            // Monitor events only if ethereum integration is enabled
                            process_nft_events(
                                &contract_set.web3,
                                &contract_set.collectible,
                                contract_set.current_block,
                                &db_pool,
                                &mut token_waitlist_map,
                            ).await.unwrap_or_else(|err| {
                                log::error!("{}", err);
                            });
                        };
                    },
                    Task::SubscriptionMonitor => {
                        if let Some(contract_set) = maybe_contract_set.as_ref() {
                            check_subscriptions(
                                &contract_set.web3,
                                &contract_set.subscription,
                                contract_set.current_block,
                                &db_pool,
                            ).await.unwrap_or_else(|err| {
                                log::error!("{}", err);
                            });
                        };
                    },
                };
                *last_run = Some(Utc::now());
            };
        }
    });
}
