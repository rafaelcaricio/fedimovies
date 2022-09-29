use serde::Deserialize;

fn default_page_size() -> u16 { 40 }

/// https://docs.joinmastodon.org/methods/instance/directory/
#[derive(Deserialize)]
pub struct DirectoryQueryParams {
    #[serde(default)]
    pub offset: u16,

    #[serde(default = "default_page_size")]
    pub limit: u16,
}
