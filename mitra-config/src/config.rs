use std::path::PathBuf;

use log::{Level as LogLevel};
use rsa::RsaPrivateKey;
use serde::Deserialize;
use url::Url;

use mitra_utils::urls::normalize_url;

use super::blockchain::{
    BlockchainConfig,
    EthereumConfig,
    MoneroConfig,
};
use super::environment::Environment;
use super::federation::FederationConfig;
use super::limits::Limits;
use super::registration::RegistrationConfig;
use super::retention::RetentionConfig;
use super::MITRA_VERSION;

fn default_log_level() -> LogLevel { LogLevel::Info }

fn default_login_message() -> String { "Do not sign this message on other sites!".to_string() }

#[derive(Clone, Deserialize)]
pub struct Config {
    // Properties auto-populated from the environment
    #[serde(skip)]
    pub environment: Environment,

    #[serde(skip)]
    pub config_path: String,

    // Core settings
    pub database_url: String,
    pub storage_dir: PathBuf,
    pub web_client_dir: Option<PathBuf>,

    pub http_host: String,
    pub http_port: u32,

    #[serde(default)]
    pub http_cors_allowlist: Vec<String>,

    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,

    // Domain name or <IP address>:<port>
    // URI scheme is optional
    instance_uri: String,

    pub instance_title: String,
    pub instance_short_description: String,
    pub instance_description: String,

    #[serde(skip)]
    pub(super) instance_rsa_key: Option<RsaPrivateKey>,

    pub(super) registrations_open: Option<bool>, // deprecated

    #[serde(default)]
    pub registration: RegistrationConfig,

    // EIP-4361 login message
    #[serde(default = "default_login_message")]
    pub login_message: String,

    pub(super) post_character_limit: Option<usize>, // deprecated

    #[serde(default)]
    pub limits: Limits,

    #[serde(default)]
    pub retention: RetentionConfig,

    pub(super) proxy_url: Option<String>,

    #[serde(default)]
    pub federation: FederationConfig,

    #[serde(default)]
    pub blocked_instances: Vec<String>,

    // Blockchain integrations
    #[serde(rename = "blockchain")]
    _blockchain: Option<BlockchainConfig>, // deprecated
    #[serde(default)]
    blockchains: Vec<BlockchainConfig>,

    // IPFS
    pub ipfs_api_url: Option<String>,
    pub ipfs_gateway_url: Option<String>,
}

impl Config {
    pub(super) fn try_instance_url(&self) -> Result<Url, url::ParseError> {
        normalize_url(&self.instance_uri)
    }

    pub fn instance(&self) -> Instance {
        Instance {
            _url: self.try_instance_url().unwrap(),
            actor_key: self.instance_rsa_key.clone().unwrap(),
            proxy_url: self.federation.proxy_url.clone(),
            onion_proxy_url: self.federation.onion_proxy_url.clone(),
            // Private instance doesn't send activities and sign requests
            is_private:
                !self.federation.enabled ||
                matches!(self.environment, Environment::Development),
            fetcher_timeout: self.federation.fetcher_timeout,
            deliverer_timeout: self.federation.deliverer_timeout,
        }
    }

    pub fn instance_url(&self) -> String {
        self.instance().url()
    }

    pub fn media_dir(&self) -> PathBuf {
        self.storage_dir.join("media")
    }

    pub fn blockchains(&self) -> &[BlockchainConfig] {
        if let Some(ref _blockchain_config) = self._blockchain {
            panic!("'blockchain' setting is not supported anymore, use 'blockchains' instead");
        } else {
            if self.blockchains.len() > 1 {
                panic!("multichain deployments are not supported");
            };
            &self.blockchains
        }
    }

    pub fn ethereum_config(&self) -> Option<&EthereumConfig> {
        self.blockchains().iter()
            .find_map(|item| match item {
                BlockchainConfig::Ethereum(config) => Some(config),
                _ => None,
            })
    }

    pub fn monero_config(&self) -> Option<&MoneroConfig> {
        self.blockchains().iter()
            .find_map(|item| match item {
                BlockchainConfig::Monero(config) => Some(config),
                _ => None,
            })
    }
}

#[derive(Clone)]
pub struct Instance {
    _url: Url,
    // Instance actor
    pub actor_key: RsaPrivateKey,
    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    pub onion_proxy_url: Option<String>,
    // Private instance won't send signed HTTP requests
    pub is_private: bool,
    pub fetcher_timeout: u64,
    pub deliverer_timeout: u64,
}

impl Instance {
    pub fn url(&self) -> String {
        self._url.origin().ascii_serialization()
    }

    pub fn hostname(&self) -> String {
        self._url.host_str().unwrap().to_string()
    }

    pub fn agent(&self) -> String {
        format!(
            "Mitra {version}; {instance_url}",
            version=MITRA_VERSION,
            instance_url=self.url(),
        )
    }
}

#[cfg(feature = "test-utils")]
impl Instance {
    pub fn for_test(url: &str) -> Self {
        use mitra_utils::crypto_rsa::generate_weak_rsa_key;
        Self {
            _url: Url::parse(url).unwrap(),
            actor_key: generate_weak_rsa_key().unwrap(),
            proxy_url: None,
            onion_proxy_url: None,
            is_private: true,
            fetcher_timeout: 0,
            deliverer_timeout: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use mitra_utils::crypto_rsa::generate_weak_rsa_key;
    use super::*;

    #[test]
    fn test_instance_url_https_dns() {
        let instance_url = Url::parse("https://example.com/").unwrap();
        let instance_rsa_key = generate_weak_rsa_key().unwrap();
        let instance = Instance {
            _url: instance_url,
            actor_key: instance_rsa_key,
            proxy_url: None,
            onion_proxy_url: None,
            is_private: true,
            fetcher_timeout: 0,
            deliverer_timeout: 0,
        };

        assert_eq!(instance.url(), "https://example.com");
        assert_eq!(instance.hostname(), "example.com");
        assert_eq!(
            instance.agent(),
            format!("Mitra {}; https://example.com", MITRA_VERSION),
        );
    }

    #[test]
    fn test_instance_url_http_ipv4() {
        let instance_url = Url::parse("http://1.2.3.4:3777/").unwrap();
        let instance_rsa_key = generate_weak_rsa_key().unwrap();
        let instance = Instance {
            _url: instance_url,
            actor_key: instance_rsa_key,
            proxy_url: None,
            onion_proxy_url: None,
            is_private: true,
            fetcher_timeout: 0,
            deliverer_timeout: 0,
        };

        assert_eq!(instance.url(), "http://1.2.3.4:3777");
        assert_eq!(instance.hostname(), "1.2.3.4");
    }
}
