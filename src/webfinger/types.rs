/// https://webfinger.net/
use std::fmt;
use std::str::FromStr;

use regex::Regex;
use serde::{Serialize, Deserialize};

use crate::errors::ValidationError;

// See also: USERNAME_RE in models::profiles::validators
const ACTOR_ADDRESS_RE: &str = r"^(?P<username>[\w\.-]+)@(?P<hostname>[\w\.-]+)$";

pub const JRD_CONTENT_TYPE: &str = "application/jrd+json";

#[derive(Deserialize)]
pub struct WebfingerQueryParams {
    pub resource: String,
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub struct ActorAddress {
    pub username: String,
    pub hostname: String,
}

impl ActorAddress {
    /// Returns acct string, as used in Mastodon
    pub fn acct(&self, local_hostname: &str) -> String {
        if self.hostname == local_hostname {
            self.username.clone()
        } else {
           self.to_string()
        }
    }
}

impl FromStr for ActorAddress {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let actor_address_re = Regex::new(ACTOR_ADDRESS_RE).unwrap();
        let caps = actor_address_re.captures(value)
            .ok_or(ValidationError("invalid actor address"))?;
        let actor_address = Self {
            username: caps["username"].to_string(),
            hostname: caps["hostname"].to_string(),
        };
        Ok(actor_address)
    }
}

impl fmt::Display for ActorAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.username, self.hostname)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Link {
    pub rel: String,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,

    pub href: Option<String>,
}

// https://datatracker.ietf.org/doc/html/rfc7033#section-4.4
#[derive(Serialize, Deserialize)]
pub struct JsonResourceDescriptor {
    pub subject: String,
    pub links: Vec<Link>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_actor_address_parse_address() {
        let value = "user_1@example.com";
        let actor_address: ActorAddress = value.parse().unwrap();
        assert_eq!(actor_address.username, "user_1");
        assert_eq!(actor_address.hostname, "example.com");
        assert_eq!(actor_address.to_string(), value);
    }

    #[test]
    fn test_actor_address_parse_mention() {
        let value = "@user_1@example.com";
        let result = value.parse::<ActorAddress>();
        assert_eq!(result.is_err(), true);
    }
}
