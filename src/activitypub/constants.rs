// https://www.w3.org/TR/activitypub/#server-to-server-interactions
pub const AP_MEDIA_TYPE: &str =
    r#"application/ld+json; profile="https://www.w3.org/ns/activitystreams""#;
pub const AS_MEDIA_TYPE: &str = "application/activity+json";

// Contexts
pub const AS_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";
pub const W3ID_SECURITY_CONTEXT: &str = "https://w3id.org/security/v1";
pub const W3ID_DATA_INTEGRITY_CONTEXT: &str = "https://w3id.org/security/data-integrity/v1";
pub const SCHEMA_ORG_CONTEXT: &str = "http://schema.org/#";
pub const MASTODON_CONTEXT: &str = "http://joinmastodon.org/ns#";

// Misc
pub const AP_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";
