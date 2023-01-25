mod blockchain;
mod environment;
mod loader;
mod main;

pub use blockchain::{
    BlockchainConfig,
    EthereumConfig,
    MoneroConfig,
};
pub use environment::Environment;
pub use loader::parse_config;
pub use main::{Config, Instance};

pub const MITRA_VERSION: &str = env!("CARGO_PKG_VERSION");
