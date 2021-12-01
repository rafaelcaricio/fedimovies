use serde::Deserialize;
use uuid::Uuid;

fn default_page_size() -> i64 { 20 }

#[derive(Deserialize)]
pub struct TimelineQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_page_size")]
    pub limit: i64,
}
