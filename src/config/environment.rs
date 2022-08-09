use std::str::FromStr;

use crate::errors::ConversionError;

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
