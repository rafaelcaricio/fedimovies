/// https://webfinger.net/
use std::{collections::HashMap, fmt, str::FromStr};

use regex::Regex;
use serde::{Deserialize, Serialize};

use mitra_models::profiles::types::DbActorProfile;

use crate::errors::ValidationError;

// See also: USERNAME_RE in validators::profiles
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
    pub fn from_mention(mention: &str) -> Result<Self, ValidationError> {
        // @ prefix is optional
        let actor_address = mention.strip_prefix('@').unwrap_or(mention).parse()?;
        Ok(actor_address)
    }

    pub fn from_profile(local_hostname: &str, profile: &DbActorProfile) -> Self {
        assert_eq!(profile.hostname.is_none(), profile.is_local());
        Self {
            username: profile.username.clone(),
            hostname: profile
                .hostname
                .as_deref()
                .unwrap_or(local_hostname)
                .to_string(),
        }
    }

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
        let caps = actor_address_re
            .captures(value)
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
    pub media_type: Option<String>,

    pub href: Option<String>,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub properties: HashMap<String, String>,
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
    use mitra_models::profiles::types::DbActor;

    #[test]
    fn test_local_actor_address() {
        let local_hostname = "example.com";
        let local_profile = DbActorProfile {
            username: "user".to_string(),
            hostname: None,
            acct: "user".to_string(),
            actor_json: None,
            ..Default::default()
        };
        let actor_address = ActorAddress::from_profile(local_hostname, &local_profile);
        assert_eq!(actor_address.to_string(), "user@example.com",);
        assert_eq!(actor_address.acct(local_hostname), local_profile.acct,);
    }

    #[test]
    fn test_remote_actor_address() {
        let local_hostname = "example.com";
        let remote_profile = DbActorProfile {
            username: "test".to_string(),
            hostname: Some("remote.com".to_string()),
            acct: "test@remote.com".to_string(),
            actor_json: Some(DbActor {
                id: "https://test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let actor_address = ActorAddress::from_profile(local_hostname, &remote_profile);
        assert_eq!(actor_address.to_string(), remote_profile.acct,);
        assert_eq!(actor_address.acct(local_hostname), remote_profile.acct,);
    }

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

    #[test]
    fn test_actor_address_from_mention() {
        let mention = "@user@example.com";
        let address_1 = ActorAddress::from_mention(mention).unwrap();
        assert_eq!(address_1.acct("example.com"), "user");

        let address_2 = ActorAddress::from_mention(mention).unwrap();
        assert_eq!(address_2.acct("server.info"), "user@example.com");

        let mention_without_prefix = "user@test.com";
        let address_3 = ActorAddress::from_mention(mention_without_prefix).unwrap();
        assert_eq!(address_3.to_string(), mention_without_prefix);

        let short_mention = "@user";
        let result = ActorAddress::from_mention(short_mention);
        assert_eq!(result.is_err(), true);
    }
}
