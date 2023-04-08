mod config;
mod environment;
mod federation;
mod limits;
mod loader;
mod registration;
mod retention;

pub use config::{Config, Instance};
pub use environment::Environment;
pub use loader::parse_config;
pub use registration::{DefaultRole, RegistrationType};

pub const REEF_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ConfigError(&'static str);
