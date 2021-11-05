use std::path::PathBuf;
use std::str::FromStr;

use serde::{de, Deserialize, Deserializer};
use url::{Url, ParseError as UrlParseError};

use crate::errors::ConversionError;

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

#[derive(Clone, Deserialize)]
pub struct EthereumContract {
    pub address: String,
    pub chain_id: u32,
    pub signing_key: String,
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

    // Instance info
    pub instance_uri: String,
    pub instance_title: String,
    pub instance_short_description: String,
    pub instance_description: String,

    #[serde(default)]
    pub registrations_open: bool, // default is false

    pub login_message: String,

    // Ethereum & IPFS
    pub ethereum_contract_dir: Option<PathBuf>,
    pub ethereum_json_rpc_url: Option<String>,
    pub ethereum_explorer_url: Option<String>,
    pub ethereum_contract: Option<EthereumContract>,
    pub ipfs_api_url: Option<String>,
    pub ipfs_gateway_url: Option<String>,
}

impl Config {
    fn try_instance_url(&self) -> Result<Url, UrlParseError> {
        // TODO: allow http in production
        let scheme = match self.environment {
            Environment::Development => "http",
            Environment::Production => "https",
        };
        let url_str = format!("{}://{}", scheme, self.instance_uri);
        Url::parse(&url_str)
    }

    pub fn instance_url(&self) -> String {
        self.try_instance_url().unwrap().origin().ascii_serialization()
    }

    pub fn media_dir(&self) -> PathBuf {
        self.storage_dir.join("media")
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
    if let Some(contract_dir) = &config.ethereum_contract_dir {
        if !contract_dir.exists() {
            panic!("contract directory does not exist");
        };
    };
    config.try_instance_url().expect("invalid instance URI");

    config
}
