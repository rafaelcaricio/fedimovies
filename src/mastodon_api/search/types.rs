use serde::{Deserialize, Serialize};

use crate::mastodon_api::accounts::types::Account;

/// https://docs.joinmastodon.org/methods/search/
#[derive(Deserialize)]
pub struct SearchQueryParams {
    pub q: String,
}

#[derive(Serialize)]
pub struct SearchResults {
    pub accounts: Vec<Account>,
}
