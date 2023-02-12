use regex::Regex;
use uuid::Uuid;

use crate::errors::ValidationError;

const ACTOR_KEY_SUFFIX: &str = "#main-key";

pub enum LocalActorCollection {
    Inbox,
    Outbox,
    Followers,
    Following,
    Subscribers,
}

impl LocalActorCollection {
    pub fn of(&self, actor_id: &str) -> String {
        let name = match self {
            Self::Inbox => "inbox",
            Self::Outbox => "outbox",
            Self::Followers => "followers",
            Self::Following => "following",
            Self::Subscribers => "subscribers",
        };
        format!("{}/{}", actor_id, name)
    }
}

// Mastodon and Pleroma use the same actor ID format
pub fn local_actor_id(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}", instance_url, username)
}

pub fn local_actor_inbox(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Inbox.of(&actor_id)
}

pub fn local_actor_outbox(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Outbox.of(&actor_id)
}

pub fn local_actor_followers(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Followers.of(&actor_id)
}

pub fn local_actor_following(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Following.of(&actor_id)
}

pub fn local_actor_subscribers(instance_url: &str, username: &str) -> String {
    let actor_id = local_actor_id(instance_url, username);
    LocalActorCollection::Subscribers.of(&actor_id)
}

pub fn local_instance_actor_id(instance_url: &str) -> String {
    format!("{}/actor", instance_url)
}

pub fn local_actor_key_id(actor_id: &str) -> String {
    format!("{}{}", actor_id, ACTOR_KEY_SUFFIX)
}

pub fn local_object_id(instance_url: &str, internal_object_id: &Uuid) -> String {
    format!("{}/objects/{}", instance_url, internal_object_id)
}

pub fn local_emoji_id(instance_url: &str, emoji_name: &str) -> String {
    format!("{}/objects/emojis/{}", instance_url, emoji_name)
}

pub fn local_tag_collection(instance_url: &str, tag_name: &str) -> String {
    format!("{}/collections/tags/{}", instance_url, tag_name)
}

pub fn parse_local_actor_id(
    instance_url: &str,
    actor_id: &str,
) -> Result<String, ValidationError> {
    let url_regexp_str = format!(
        "^{}/users/(?P<username>[0-9a-z_]+)$",
        instance_url.replace('.', r"\."),
    );
    let url_regexp = Regex::new(&url_regexp_str)
        .map_err(|_| ValidationError("error"))?;
    let url_caps = url_regexp.captures(actor_id)
        .ok_or(ValidationError("invalid actor ID"))?;
    let username = url_caps.name("username")
        .ok_or(ValidationError("invalid actor ID"))?
        .as_str()
        .to_owned();
    Ok(username)
}

pub fn parse_local_object_id(
    instance_url: &str,
    object_id: &str,
) -> Result<Uuid, ValidationError> {
    let url_regexp_str = format!(
        "^{}/objects/(?P<uuid>[0-9a-f-]+)$",
        instance_url.replace('.', r"\."),
    );
    let url_regexp = Regex::new(&url_regexp_str)
        .map_err(|_| ValidationError("error"))?;
    let url_caps = url_regexp.captures(object_id)
        .ok_or(ValidationError("invalid object ID"))?;
    let internal_object_id: Uuid = url_caps.name("uuid")
        .ok_or(ValidationError("invalid object ID"))?
        .as_str().parse()
        .map_err(|_| ValidationError("invalid object ID"))?;
    Ok(internal_object_id)
}

#[cfg(test)]
mod tests {
    use crate::utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.org";

    #[test]
    fn test_parse_local_actor_id() {
        let username = parse_local_actor_id(
            INSTANCE_URL,
            "https://example.org/users/test",
        ).unwrap();
        assert_eq!(username, "test".to_string());
    }

    #[test]
    fn test_parse_local_actor_id_wrong_path() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://example.org/user/test",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_invalid_username() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://example.org/users/tes-t",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_local_actor_id_invalid_instance_url() {
        let error = parse_local_actor_id(
            INSTANCE_URL,
            "https://example.gov/users/test",
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_local_object_id() {
        let expected_uuid = generate_ulid();
        let object_id = format!(
            "https://example.org/objects/{}",
            expected_uuid,
        );
        let internal_object_id = parse_local_object_id(
            INSTANCE_URL,
            &object_id,
        ).unwrap();
        assert_eq!(internal_object_id, expected_uuid);
    }

    #[test]
    fn test_parse_local_object_id_invalid_uuid() {
        let object_id = "https://example.org/objects/1234";
        let error = parse_local_object_id(
            INSTANCE_URL,
            object_id,
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid object ID");
    }
}
