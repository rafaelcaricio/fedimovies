/// https://docs.joinmastodon.org/methods/search/
use serde::{Deserialize, Serialize};

use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::statuses::types::Status;

fn default_limit() -> i64 { 20 }

#[derive(Deserialize)]
pub struct SearchQueryParams {
    pub q: String,

    #[serde(default = "default_limit")]
    pub limit: i64,
}

#[derive(Serialize)]
pub struct SearchResults {
    pub accounts: Vec<Account>,
    pub statuses: Vec<Status>,
}
