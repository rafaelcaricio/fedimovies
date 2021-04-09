use std::time::Duration;

use crate::config::Config;
use crate::database::Pool;
use crate::ethereum::nft::{get_nft_contract, process_events};

pub fn run(config: Config, db_pool: Pool) -> () {
    actix_rt::spawn(async move {
        let mut interval = actix_rt::time::interval(Duration::from_secs(30));
        // Verify config and create contract interface
        let web3_contract = get_nft_contract(&config).await
            .map_err(|err| log::error!("{}", err))
            .ok();
        loop {
            interval.tick().await;
            // Process events only if contract is properly configured
            if let Some((web3, contract)) = web3_contract.as_ref() {
                process_events(web3, contract, &db_pool).await.unwrap_or_else(|err| {
                    log::error!("{}", err);
                });
            }
        }
    });
}
