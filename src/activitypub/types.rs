use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

fn default_tag_type() -> String { HASHTAG.to_string() }

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
    pub tag: Option<Value>,
    pub to: Option<Value>,
    pub updated: Option<DateTime<Utc>>,
    pub url: Option<Value>,
}
