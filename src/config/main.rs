use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use log::{Level as LogLevel};
use rsa::RsaPrivateKey;
use serde::Deserialize;
use url::Url;

use crate::activitypub::constants::ACTOR_KEY_SUFFIX;
use crate::activitypub::identifiers::local_instance_actor_id;
use crate::errors::ConversionError;
use crate::utils::crypto::{
    deserialize_private_key,
    generate_private_key,
    serialize_private_key,
};
use crate::utils::files::{set_file_permissions, write_file};

use super::blockchain::BlockchainConfig;
use super::environment::Environment;

struct EnvConfig {
    environment: Environment,
    config_path: String,
    crate_version: String,
}

#[cfg(feature = "production")]
const DEFAULT_CONFIG_PATH: &str = "/etc/mitra/config.yaml";
#[cfg(not(feature = "production"))]
const DEFAULT_CONFIG_PATH: &str = "config.yaml";

fn parse_env() -> EnvConfig {
    dotenv::from_filename(".env.local").ok();
    dotenv::dotenv().ok();
    let environment_str = std::env::var("ENVIRONMENT").ok();
    let environment = environment_str
        .map(|val| Environment::from_str(&val).expect("invalid environment type"))
        .unwrap_or_default();
    let config_path = std::env::var("CONFIG_PATH")
        .unwrap_or(DEFAULT_CONFIG_PATH.to_string());
    let crate_version = env!("CARGO_PKG_VERSION").to_string();
    EnvConfig {
        environment,
        config_path,
        crate_version,
    }
}

fn default_log_level() -> LogLevel { LogLevel::Info }

fn default_login_message() -> String { "Do not sign this message on other sites!".to_string() }

fn default_post_character_limit() -> usize { 2000 }

#[derive(Clone, Deserialize)]
pub struct Config {
    #[serde(skip)]
    pub environment: Environment,

    #[serde(skip)]
    pub config_path: String,

    #[serde(skip)]
    pub version: String,

    // Core settings
    pub database_url: String,
    pub storage_dir: PathBuf,

    pub http_host: String,
    pub http_port: u32,

    #[serde(default)]
    pub http_cors_allowlist: Vec<String>,

    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,

    // Domain name or <IP address>:<port>
    instance_uri: String,
    pub instance_title: String,
    pub instance_short_description: String,
    pub instance_description: String,

    #[serde(skip)]
    instance_rsa_key: Option<RsaPrivateKey>,

    #[serde(default)]
    pub registrations_open: bool, // default is false

    // EIP-4361 login message
    #[serde(default = "default_login_message")]
    pub login_message: String,

    #[serde(default = "default_post_character_limit")]
    pub post_character_limit: usize,

    #[serde(default)]
    pub blocked_instances: Vec<String>,

    // Blockchain integrations
    #[serde(rename = "blockchain")]
    pub _blockchain: Option<BlockchainConfig>, // deprecated
    #[serde(default)]
    blockchains: Vec<BlockchainConfig>,

    // IPFS
    pub ipfs_api_url: Option<String>,
    pub ipfs_gateway_url: Option<String>,
}

impl Config {
    fn try_instance_url(&self) -> Result<Url, ConversionError> {
        // TODO: allow http in production
        let scheme = match self.environment {
            Environment::Development => "http",
            Environment::Production => "https",
        };
        let url_str = format!("{}://{}", scheme, self.instance_uri);
        let url = Url::parse(&url_str).map_err(|_| ConversionError)?;
        url.host().ok_or(ConversionError)?; // validates URL
        Ok(url)
    }

    pub fn instance(&self) -> Instance {
        Instance {
            _url: self.try_instance_url().unwrap(),
            _version: self.version.clone(),
            actor_key: self.instance_rsa_key.clone().unwrap(),
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
        if let Some(ref blockchain_config) = self._blockchain {
            Some(blockchain_config)
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
    _version: String,
    // Instance actor
    pub actor_key: RsaPrivateKey,
    // Private instance won't send signed HTTP requests
    pub is_private: bool,
}

impl Instance {
    #[cfg(test)]
    pub fn new(url: Url, actor_key: RsaPrivateKey) -> Self {
        Self {
            _url: url,
            _version: "0.0.0".to_string(),
            actor_key,
            is_private: true,
        }
    }

    pub fn url(&self) -> String {
        self._url.origin().ascii_serialization()
    }

    pub fn host(&self) -> String {
        self._url.host_str().unwrap().to_string()
    }

    pub fn actor_id(&self) -> String {
        local_instance_actor_id(&self.url())
    }

    pub fn actor_key_id(&self) -> String {
        format!("{}{}", self.actor_id(), ACTOR_KEY_SUFFIX)
    }

    pub fn agent(&self) -> String {
        format!(
            "Mitra {version}; {instance_url}",
            version=self._version,
            instance_url=self.url(),
        )
    }
}

extern "C" {
    fn geteuid() -> u32;
}

fn check_directory_owner(path: &Path) -> () {
    let metadata = std::fs::metadata(path)
        .expect("can't read file metadata");
    let current_uid = unsafe { geteuid() };
    if metadata.uid() != current_uid {
        panic!("directory owner is not the current user");
    };
}

/// Generates new instance RSA key or returns existing key
fn read_instance_rsa_key(storage_dir: &Path) -> RsaPrivateKey {
    let private_key_path = storage_dir.join("instance_rsa_key");
    if private_key_path.exists() {
        let private_key_str = std::fs::read_to_string(&private_key_path)
            .expect("failed to read instance RSA key");
        let private_key = deserialize_private_key(&private_key_str)
            .expect("failed to read instance RSA key");
        private_key
    } else {
        let private_key = generate_private_key()
            .expect("failed to generate RSA key");
        let private_key_str = serialize_private_key(&private_key)
            .expect("failed to serialize RSA key");
        write_file(private_key_str.as_bytes(), &private_key_path)
            .expect("failed to write instance RSA key");
        set_file_permissions(&private_key_path, 0o600)
            .expect("failed to set permissions on RSA key file");
        private_key
    }
}

pub fn parse_config() -> Config {
    let env = parse_env();
    let config_yaml = std::fs::read_to_string(&env.config_path)
        .expect("failed to load config file");
    let mut config = serde_yaml::from_str::<Config>(&config_yaml)
        .expect("invalid yaml data");
    // Set parameters from environment
    config.environment = env.environment;
    config.config_path = env.config_path;
    config.version = env.crate_version;

    // Validate config
    if !config.storage_dir.exists() {
        panic!("storage directory does not exist");
    };
    check_directory_owner(&config.storage_dir);
    config.try_instance_url().expect("invalid instance URI");
    if let Some(blockchain_config) = config.blockchain() {
        if let Some(ethereum_config) = blockchain_config.ethereum_config() {
            ethereum_config.try_ethereum_chain_id().unwrap();
            if !ethereum_config.contract_dir.exists() {
                panic!("contract directory does not exist");
            };
        };
    };
    if config.ipfs_api_url.is_some() != config.ipfs_gateway_url.is_some() {
        panic!("both ipfs_api_url and ipfs_gateway_url must be set");
    };

    // Insert instance RSA key
    config.instance_rsa_key = Some(read_instance_rsa_key(&config.storage_dir));

    config
}

#[cfg(test)]
mod tests {
    use crate::utils::crypto::generate_weak_private_key;
    use super::*;

    #[test]
    fn test_instance_url_https_dns() {
        let instance_url = Url::parse("https://example.com/").unwrap();
        let instance_rsa_key = generate_weak_private_key().unwrap();
        let instance = Instance {
            _url: instance_url,
            _version: "1.0.0".to_string(),
            actor_key: instance_rsa_key,
            is_private: true,
        };

        assert_eq!(instance.url(), "https://example.com");
        assert_eq!(instance.host(), "example.com");
        assert_eq!(instance.agent(), "Mitra 1.0.0; https://example.com");
    }

    #[test]
    fn test_instance_url_http_ipv4() {
        let instance_url = Url::parse("http://1.2.3.4:3777/").unwrap();
        let instance_rsa_key = generate_weak_private_key().unwrap();
        let instance = Instance {
            _url: instance_url,
            _version: "1.0.0".to_string(),
            actor_key: instance_rsa_key,
            is_private: true,
        };

        assert_eq!(instance.url(), "http://1.2.3.4:3777");
        assert_eq!(instance.host(), "1.2.3.4");
    }
}
