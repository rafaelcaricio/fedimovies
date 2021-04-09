use serde::Serialize;

use crate::mastodon_api::accounts::types::Account;

/// https://docs.joinmastodon.org/methods/search/
#[derive(Serialize)]
pub struct SearchResults {
    pub accounts: Vec<Account>,
}
