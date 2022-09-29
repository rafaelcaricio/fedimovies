use serde::Deserialize;

use crate::mastodon_api::pagination::PageSize;

fn default_page_size() -> PageSize { PageSize::new(40) }

/// https://docs.joinmastodon.org/methods/instance/directory/
#[derive(Deserialize)]
pub struct DirectoryQueryParams {
    #[serde(default)]
    pub offset: u16,

    #[serde(default = "default_page_size")]
    pub limit: PageSize,
}
