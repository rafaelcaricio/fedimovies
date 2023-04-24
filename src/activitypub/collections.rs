use serde::Serialize;
use serde_json::Value as JsonValue;

use super::types::{build_default_context, Context};
use super::vocabulary::{ORDERED_COLLECTION, ORDERED_COLLECTION_PAGE};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedCollection {
    #[serde(rename = "@context")]
    pub context: Context,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    first: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    total_items: Option<i32>,
}

impl OrderedCollection {
    pub fn new(
        collection_id: String,
        first_page_id: Option<String>,
        total_items: Option<i32>,
    ) -> Self {
        Self {
            context: build_default_context(),
            id: collection_id,
            object_type: ORDERED_COLLECTION.to_string(),
            first: first_page_id,
            total_items,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedCollectionPage {
    #[serde(rename = "@context")]
    pub context: Context,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    ordered_items: Vec<JsonValue>,
}

impl OrderedCollectionPage {
    pub fn new(collection_page_id: String, items: Vec<JsonValue>) -> Self {
        Self {
            context: build_default_context(),
            id: collection_page_id,
            object_type: ORDERED_COLLECTION_PAGE.to_string(),
            ordered_items: items,
        }
    }
}
