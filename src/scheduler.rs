use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::config::Config;
use crate::database::Pool;
use crate::ethereum::contracts::get_contracts;
use crate::ethereum::nft::process_nft_events;

pub fn run(config: Config, db_pool: Pool) -> () {
    actix_rt::spawn(async move {
        let mut interval = actix_rt::time::interval(Duration::from_secs(30));
        let maybe_contract_set = if let Some(blockchain_config) = &config.blockchain {
            // Create blockchain interface
            get_contracts(blockchain_config).await
                .map_err(|err| log::error!("{}", err))
                .ok()
        } else {
            None
        };
        let mut token_waitlist_map: HashMap<Uuid, DateTime<Utc>> = HashMap::new();
        loop {
            interval.tick().await;

            if let Some(contract_set) = maybe_contract_set.as_ref() {
                // Monitor events only if ethereum integration is enabled
                process_nft_events(
                    &contract_set.web3,
                    &contract_set.collectible,
                    &db_pool,
                    &mut token_waitlist_map,
                ).await.unwrap_or_else(|err| {
                    log::error!("{}", err);
                });
            }
        }
    });
}
