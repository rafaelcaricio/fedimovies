use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use fedimovies_models::markers::types::{DbTimelineMarker, Timeline};

use crate::errors::ValidationError;

#[derive(Deserialize)]
pub struct MarkerQueryParams {
    #[serde(rename = "timeline[]")]
    pub timeline: String,
}

impl MarkerQueryParams {
    pub fn to_timeline(&self) -> Result<Timeline, ValidationError> {
        let timeline = match self.timeline.as_ref() {
            "home" => Timeline::Home,
            "notifications" => Timeline::Notifications,
            _ => return Err(ValidationError("invalid timeline name")),
        };
        Ok(timeline)
    }
}

#[derive(Deserialize)]
pub struct MarkerCreateData {
    #[serde(rename = "notifications[last_read_id]")]
    pub notifications: String,
}

/// https://docs.joinmastodon.org/entities/marker/
#[derive(Serialize)]
pub struct Marker {
    last_read_id: String,
    version: i32,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Markers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications: Option<Marker>,
}

impl From<DbTimelineMarker> for Marker {
    fn from(value: DbTimelineMarker) -> Self {
        Self {
            last_read_id: value.last_read_id,
            version: 0,
            updated_at: value.updated_at,
        }
    }
}
