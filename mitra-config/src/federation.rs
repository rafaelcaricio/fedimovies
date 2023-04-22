use serde::Deserialize;

fn default_federation_enabled() -> bool { true }

const fn default_fetcher_timeout() -> u64 { 300 }
const fn default_deliverer_timeout() -> u64 { 30 }

#[derive(Clone, Deserialize)]
pub struct FederationConfig {
    #[serde(default = "default_federation_enabled")]
    pub enabled: bool,
    #[serde(default = "default_fetcher_timeout")]
    pub(super) fetcher_timeout: u64,
    #[serde(default = "default_deliverer_timeout")]
    pub(super) deliverer_timeout: u64,
    pub(super) proxy_url: Option<String>,
    pub(super) onion_proxy_url: Option<String>,
    pub(super) i2p_proxy_url: Option<String>,
}

impl Default for FederationConfig {
    fn default() -> Self {
        Self {
            enabled: default_federation_enabled(),
            fetcher_timeout: default_fetcher_timeout(),
            deliverer_timeout: default_deliverer_timeout(),
            proxy_url: None,
            onion_proxy_url: None,
            i2p_proxy_url: None,
        }
    }
}
