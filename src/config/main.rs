use std::path::PathBuf;

use log::{Level as LogLevel};
use rsa::RsaPrivateKey;
use serde::{
    Deserialize,
    Deserializer,
    de::Error as DeserializerError,
};
use url::Url;

use mitra_utils::urls::normalize_url;

use super::blockchain::BlockchainConfig;
use super::environment::Environment;
use super::limits::Limits;
use super::retention::RetentionConfig;
use super::MITRA_VERSION;

#[derive(Clone, PartialEq)]
pub enum RegistrationType {
    Open,
    Invite,
}

impl Default for RegistrationType {
    fn default() -> Self { Self::Invite }
}

impl<'de> Deserialize<'de> for RegistrationType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let registration_type_str = String::deserialize(deserializer)?;
        let registration_type = match registration_type_str.as_str() {
            "open" => Self::Open,
            "invite" => Self::Invite,
            _ => return Err(DeserializerError::custom("unknown registration type")),
        };
        Ok(registration_type)
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct RegistrationConfig {
    #[serde(rename = "type")]
    pub registration_type: RegistrationType,

    #[serde(default)]
    pub default_role_read_only_user: bool, // default is false
}

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

    proxy_url: Option<String>,

    #[serde(default)]
    pub limits: Limits,

    #[serde(default)]
    pub retention: RetentionConfig,

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
            proxy_url: self.proxy_url.clone(),
            is_private: matches!(self.environment, Environment::Development),
        }
    }

    pub fn instance_url(&self) -> String {
        self.instance().url()
    }

    pub fn media_dir(&self) -> PathBuf {
        self.storage_dir.join("media")
    }

    pub fn blockchain(&self) -> Option<&BlockchainConfig> {
        if let Some(ref _blockchain_config) = self._blockchain {
            panic!("'blockchain' setting is not supported anymore, use 'blockchains' instead");
        } else {
            match &self.blockchains[..] {
                [blockchain_config] => Some(blockchain_config),
                [] => None,
                _ => panic!("multichain deployments are not supported"),
            }
        }
    }
}

#[derive(Clone)]
pub struct Instance {
    _url: Url,
    // Instance actor
    pub actor_key: RsaPrivateKey,
    // Proxy for outgoing requests
    pub proxy_url: Option<String>,
    // Private instance won't send signed HTTP requests
    pub is_private: bool,
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

#[cfg(test)]
impl Instance {
    pub fn for_test(url: &str) -> Self {
        use mitra_utils::crypto_rsa::generate_weak_rsa_key;
        Self {
            _url: Url::parse(url).unwrap(),
            actor_key: generate_weak_rsa_key().unwrap(),
            proxy_url: None,
            is_private: true,
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
            is_private: true,
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
            is_private: true,
        };

        assert_eq!(instance.url(), "http://1.2.3.4:3777");
        assert_eq!(instance.hostname(), "1.2.3.4");
    }
}
