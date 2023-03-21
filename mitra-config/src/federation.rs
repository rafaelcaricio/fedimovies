use serde::Deserialize;

fn default_federation_enabled() -> bool { true }

#[derive(Clone, Deserialize)]
pub struct FederationConfig {
    #[serde(default = "default_federation_enabled")]
    pub enabled: bool,
    pub(super) proxy_url: Option<String>,
    pub(super) onion_proxy_url: Option<String>,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: default_federation_enabled(),
            proxy_url: None,
            onion_proxy_url: None,
        }
    }
}
