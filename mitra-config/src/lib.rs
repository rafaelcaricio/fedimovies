mod blockchain;
mod config;
mod environment;
mod limits;
mod loader;
mod retention;

pub use blockchain::{
    BlockchainConfig,
    EthereumConfig,
    MoneroConfig,
};
pub use config::{Config, Instance, RegistrationType};
pub use environment::Environment;
pub use loader::parse_config;

pub const MITRA_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ConfigError(&'static str);
