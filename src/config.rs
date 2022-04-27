use std::path::PathBuf;
use std::str::FromStr;

use log::{Level as LogLevel};
use rsa::RsaPrivateKey;
use serde::{de, Deserialize, Deserializer};
use url::Url;

use crate::activitypub::views::get_instance_actor_url;
use crate::errors::ConversionError;
use crate::ethereum::utils::{parse_caip2_chain_id, ChainIdError};
use crate::models::profiles::currencies::Currency;
use crate::utils::crypto::deserialize_private_key;

#[derive(Clone, Debug)]
pub enum Environment {
    Development,
    Production,
}

impl FromStr for Environment {
    type Err = ConversionError;

    fn from_str(val: &str) -> Result<Self, Self::Err> {
        let environment = match val {
            "development" => Environment::Development,
            "production" => Environment::Production,
            _ => return Err(ConversionError),
        };
        Ok(environment)
    }
}

fn environment_from_str<'de, D>(deserializer: D) -> Result<Environment, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Environment::from_str(&s).map_err(de::Error::custom)
}

#[derive(Clone)]
pub struct EnvConfig {
    pub environment: Option<Environment>,
    pub config_path: String,
    pub crate_version: String,
}

fn parse_env() -> EnvConfig {
    dotenv::from_filename(".env.local").ok();
    dotenv::dotenv().ok();
    let environment_str = std::env::var("ENVIRONMENT").ok();
    let environment = environment_str
        .map(|val| Environment::from_str(&val).expect("invalid environment type"));
    let config_path = std::env::var("CONFIG_PATH")
        .unwrap_or("config.yaml".to_string());
    let crate_version = env!("CARGO_PKG_VERSION").to_string();
    EnvConfig {
        environment,
        config_path,
        crate_version,
    }
}

fn default_environment() -> Environment { Environment::Development }

fn default_log_level() -> LogLevel { LogLevel::Info }

fn default_post_character_limit() -> usize { 2000 }

#[derive(Clone, Deserialize)]
pub struct BlockchainConfig {
    pub chain_id: String,
    pub contract_address: String,
    pub contract_dir: PathBuf,
    pub api_url: String,
    pub explorer_url: Option<String>,
    pub signing_key: String,
}

impl BlockchainConfig {
    fn try_ethereum_chain_id(&self) -> Result<u32, ChainIdError> {
        parse_caip2_chain_id(&self.chain_id)
    }

    pub fn ethereum_chain_id(&self) -> u32 {
        self.try_ethereum_chain_id().unwrap()
    }
}

#[derive(Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_environment")]
    #[serde(deserialize_with = "environment_from_str")]
    pub environment: Environment,

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

    // Instance info
    instance_uri: String,
    pub instance_title: String,
    pub instance_short_description: String,
    pub instance_description: String,
    instance_rsa_key: String,

    #[serde(default)]
    pub registrations_open: bool, // default is false

    pub login_message: String,

    #[serde(default = "default_post_character_limit")]
    pub post_character_limit: usize,

    #[serde(default)]
    pub blocked_instances: Vec<String>,

    // Blockchain integration
    pub blockchain: Option<BlockchainConfig>,

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

    fn try_instance_rsa_key(&self) -> Result<RsaPrivateKey, rsa::pkcs8::Error> {
        deserialize_private_key(&self.instance_rsa_key)
    }

    pub fn instance(&self) -> Instance {
        Instance {
            _url: self.try_instance_url().unwrap(),
            _version: self.version.clone(),
            actor_key: self.try_instance_rsa_key().unwrap(),
            is_private: matches!(self.environment, Environment::Development),
        }
    }

    pub fn instance_url(&self) -> String {
        self.instance().url()
    }

    pub fn media_dir(&self) -> PathBuf {
        self.storage_dir.join("media")
    }

    pub fn default_currency(&self) -> Currency {
        Currency::Ethereum
    }
}

pub struct Instance {
    _url: Url,
    _version: String,
    // Instance actor
    pub actor_key: RsaPrivateKey,
    // Private instance won't send signed HTTP requests
    pub is_private: bool,
}

impl Instance {

    pub fn url(&self) -> String {
        self._url.origin().ascii_serialization()
    }

    pub fn host(&self) -> String {
        self._url.host_str().unwrap().to_string()
    }

    pub fn actor_id(&self) -> String {
        get_instance_actor_url(&self.url())
    }

    pub fn actor_key_id(&self) -> String {
        format!("{}#main-key", self.actor_id())
    }

    pub fn agent(&self) -> String {
        format!(
            "Mitra {version}; {instance_url}",
            version=self._version,
            instance_url=self.url(),
        )
    }
}

pub fn parse_config() -> Config {
    let env = parse_env();
    let config_yaml = std::fs::read_to_string(env.config_path)
        .expect("failed to load config file");
    let mut config = serde_yaml::from_str::<Config>(&config_yaml)
        .expect("invalid yaml data");
    // Override environment parameter in config if env variable is set
    config.environment = env.environment.unwrap_or(config.environment);
    // Set_version
    config.version = env.crate_version;
    // Validate config
    if !config.storage_dir.exists() {
        panic!("storage directory does not exist");
    };
    if let Some(blockchain_config) = config.blockchain.as_ref() {
        blockchain_config.try_ethereum_chain_id().unwrap();
        if !blockchain_config.contract_dir.exists() {
            panic!("contract directory does not exist");
        };
    };
    config.try_instance_url().expect("invalid instance URI");
    config.try_instance_rsa_key().expect("invalid instance RSA key");
    if config.ipfs_api_url.is_some() != config.ipfs_gateway_url.is_some() {
        panic!("both ipfs_api_url and ipfs_gateway_url must be set");
    };

    config
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use super::*;

    #[test]
    fn test_instance_url_https_dns() {
        let instance_url = Url::parse("https://example.com/").unwrap();
        let instance_rsa_key = RsaPrivateKey::new(&mut OsRng, 512).unwrap();
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
        let instance_rsa_key = RsaPrivateKey::new(&mut OsRng, 512).unwrap();
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
