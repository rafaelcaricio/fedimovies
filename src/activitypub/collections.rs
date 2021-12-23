use serde::Serialize;
use serde_json::{json, Value};

use super::constants::AP_CONTEXT;
use super::vocabulary::{ORDERED_COLLECTION, ORDERED_COLLECTION_PAGE};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedCollection {
    #[serde(rename = "@context")]
    pub context: Value,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    first: Option<String>,
}

impl OrderedCollection {
    pub fn new(
        collection_id: String,
        first_page_id: Option<String>,
    ) -> Self {
        Self {
            context: json!(AP_CONTEXT),
            id: collection_id,
            object_type: ORDERED_COLLECTION.to_string(),
            first: first_page_id,
        }
    }
}

pub const COLLECTION_PAGE_SIZE: i64 = 10;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedCollectionPage {
    #[serde(rename = "@context")]
    pub context: Value,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    ordered_items: Vec<Value>,
}

impl OrderedCollectionPage {
    pub fn new(
        collection_page_id: String,
        items: Vec<impl Serialize>,
    ) -> Self {
        let ordered_items = items.into_iter()
            .map(|item| json!(item)).collect();
        Self {
            context: json!(AP_CONTEXT),
            id: collection_page_id,
            object_type: ORDERED_COLLECTION_PAGE.to_string(),
            ordered_items,
        }
    }
}
