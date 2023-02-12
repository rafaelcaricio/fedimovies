/// https://docs.joinmastodon.org/methods/search/
use serde::{Deserialize, Serialize};

use crate::mastodon_api::{
    accounts::types::Account,
    pagination::PageSize,
    statuses::types::{Status, Tag},
};

fn default_page_size() -> PageSize { PageSize::new(20) }

#[derive(Deserialize)]
pub struct SearchQueryParams {
    pub q: String,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}

#[derive(Serialize)]
pub struct SearchResults {
    pub accounts: Vec<Account>,
    pub statuses: Vec<Status>,
    pub hashtags: Vec<Tag>,
}
