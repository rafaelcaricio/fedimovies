use std::str::FromStr;

use super::ConfigError;

#[derive(Clone, Debug)]
pub enum Environment {
    Development,
    Production,
}

impl Default for Environment {
    #[cfg(feature = "production")]
    fn default() -> Self { Self::Production }
    #[cfg(not(feature = "production"))]
    fn default() -> Self { Self::Development }
}

impl FromStr for Environment {
    type Err = ConfigError;

    fn from_str(val: &str) -> Result<Self, Self::Err> {
        let environment = match val {
            "development" => Environment::Development,
            "production" => Environment::Production,
            _ => return Err(ConfigError("invalid environment type")),
        };
        Ok(environment)
    }
}
