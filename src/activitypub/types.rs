use crate::activitypub::constants::{
    AS_CONTEXT, MASTODON_CONTEXT, SCHEMA_ORG_CONTEXT, W3ID_SECURITY_CONTEXT,
};
use chrono::{DateTime, Utc};
use serde::{de::Error as DeserializerError, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

use super::receiver::parse_property_value;
use super::vocabulary::HASHTAG;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    #[serde(rename = "type")]
    pub attachment_type: String,

    pub name: Option<String>,
    pub media_type: Option<String>,
    pub url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub href: String,
}

fn default_tag_type() -> String {
    HASHTAG.to_string()
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: Option<String>,

    #[serde(rename = "type", default = "default_tag_type")]
    pub tag_type: String,

    pub href: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub href: String,
    pub name: String,
}

/// https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-e232.md
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub href: String,
    pub media_type: String,
    pub name: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiTagImage {
    #[serde(rename = "type")]
    pub object_type: String,
    pub url: String,
    pub media_type: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub icon: EmojiTagImage,
    pub id: String,
    pub name: String,
    pub updated: DateTime<Utc>,
}

pub fn deserialize_value_array<'de, D>(deserializer: D) -> Result<Vec<Value>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_value: Option<Value> = Option::deserialize(deserializer)?;
    let values = if let Some(value) = maybe_value {
        parse_property_value(&value).map_err(DeserializerError::custom)?
    } else {
        vec![]
    };
    Ok(values)
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Object {
    // https://www.w3.org/TR/activitypub/#obj-id
    // "id" and "type" are required properties
    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    pub name: Option<String>,
    pub attachment: Option<Value>,
    pub cc: Option<Value>,
    pub media_type: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub attributed_to: Option<Value>,
    pub in_reply_to: Option<String>,
    pub content: Option<String>,
    pub quote_url: Option<String>,
    pub sensitive: Option<bool>,

    #[serde(default, deserialize_with = "deserialize_value_array")]
    pub tag: Vec<Value>,

    pub to: Option<Value>,
    pub updated: Option<DateTime<Utc>>,
    pub url: Option<Value>,
}

pub type Context = Value;

pub fn build_default_context() -> Context {
    json!([
        AS_CONTEXT,
        W3ID_SECURITY_CONTEXT,
        {
            "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
            "toot": MASTODON_CONTEXT,
            "featured": {
                "@id": "toot:featured",
                "@type": "@id"
            },
            "featuredTags": {
                "@id": "toot:featuredTags",
                "@type": "@id"
            },
            "alsoKnownAs": {
                "@id": "as:alsoKnownAs",
                "@type": "@id"
            },
            "movedTo": {
                "@id": "as:movedTo",
                "@type": "@id"
            },
            "schema": SCHEMA_ORG_CONTEXT,
            "PropertyValue": "schema:PropertyValue",
            "value": "schema:value",
            "IdentityProof": "toot:IdentityProof",
            "discoverable": "toot:discoverable",
            "Device": "toot:Device",
            "Ed25519Signature": "toot:Ed25519Signature",
            "Ed25519Key": "toot:Ed25519Key",
            "Curve25519Key": "toot:Curve25519Key",
            "EncryptedMessage": "toot:EncryptedMessage",
            "publicKeyBase64": "toot:publicKeyBase64",
            "deviceId": "toot:deviceId",
            "claim": {
                "@type": "@id",
                "@id": "toot:claim"
            },
            "fingerprintKey": {
                "@type": "@id",
                "@id": "toot:fingerprintKey"
            },
            "identityKey": {
                "@type": "@id",
                "@id": "toot:identityKey"
            },
            "devices": {
                "@type": "@id",
                "@id": "toot:devices"
            },
            "messageFranking": "toot:messageFranking",
            "messageType": "toot:messageType",
            "cipherText": "toot:cipherText",
            "suspended": "toot:suspended",
            "focalPoint": {
                "@container": "@list",
                "@id": "toot:focalPoint"
            }
        }
    ])
}
