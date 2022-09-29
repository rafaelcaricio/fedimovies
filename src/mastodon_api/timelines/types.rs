use serde::Deserialize;
use uuid::Uuid;

use crate::mastodon_api::pagination::PageSize;

fn default_page_size() -> PageSize { PageSize::new(20) }

#[derive(Deserialize)]
pub struct TimelineQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}
