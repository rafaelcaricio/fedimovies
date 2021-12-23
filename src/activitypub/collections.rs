use serde::Serialize;
use serde_json::{json, Value};

use super::constants::AP_CONTEXT;
use super::vocabulary::ORDERED_COLLECTION;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedCollection {
    #[serde(rename = "@context")]
    pub context: Value,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,
}

impl OrderedCollection {
    pub fn new(collection_id: String) -> Self {
        Self {
            context: json!(AP_CONTEXT),
            id: collection_id,
            object_type: ORDERED_COLLECTION.to_string(),
        }
    }
}
