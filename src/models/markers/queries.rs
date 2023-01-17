use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};
use super::types::{DbTimelineMarker, Timeline};

pub async fn create_or_update_marker(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
    timeline: Timeline,
    last_read_id: String,
) -> Result<DbTimelineMarker, DatabaseError> {
    let row = db_client.query_one(
        "
        INSERT INTO timeline_marker (user_id, timeline, last_read_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, timeline) DO UPDATE
        SET last_read_id = $3, updated_at = now()
        RETURNING timeline_marker
        ",
        &[&user_id, &timeline, &last_read_id],
    ).await?;
    let marker = row.try_get("timeline_marker")?;
    Ok(marker)
}

pub async fn get_marker_opt(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
    timeline: Timeline,
) -> Result<Option<DbTimelineMarker>, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT timeline_marker
        FROM timeline_marker
        WHERE user_id = $1 AND timeline = $2
        ",
        &[&user_id, &timeline],
    ).await?;
    let maybe_marker = match maybe_row {
        Some(row) => row.try_get("timeline_marker")?,
        None => None,
    };
    Ok(maybe_marker)
}
