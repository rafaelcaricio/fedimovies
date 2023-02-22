use serde::Deserialize;

#[derive(Clone, Default, Deserialize)]
pub struct FederationConfig {
    pub proxy_url: Option<String>,
}
