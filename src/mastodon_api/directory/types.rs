use serde::Deserialize;

fn default_page_size() -> i64 { 40 }

/// https://docs.joinmastodon.org/methods/instance/directory/
#[derive(Deserialize)]
pub struct DirectoryQueryParams {
    #[serde(default)]
    pub offset: i64,

    #[serde(default = "default_page_size")]
    pub limit: i64,
}
