use serde::Deserialize;
use uuid::Uuid;

fn default_page_size() -> u16 { 20 }

#[derive(Deserialize)]
pub struct TimelineQueryParams {
    pub max_id: Option<Uuid>,

    #[serde(default = "default_page_size")]
    pub limit: u16,
}
