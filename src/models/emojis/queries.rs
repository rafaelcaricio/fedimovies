use chrono::{DateTime, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::{catch_unique_violation, DatabaseError};
use crate::models::{
    instances::queries::create_instance,
    profiles::types::ProfileImage,
};
use crate::utils::id::new_uuid;
use super::types::DbEmoji;

pub async fn create_emoji(
    db_client: &impl GenericClient,
    emoji_name: &str,
    hostname: Option<&str>,
    file_name: &str,
    media_type: &str,
    object_id: Option<&str>,
    updated_at: &DateTime<Utc>,
) -> Result<DbEmoji, DatabaseError> {
    let emoji_id = new_uuid();
    if let Some(hostname) = hostname {
        create_instance(db_client, hostname).await?;
    };
    let image = ProfileImage {
        file_name: file_name.to_string(),
        media_type: Some(media_type.to_string()),
    };
    let row = db_client.query_one(
        "
        INSERT INTO emoji (
            id,
            emoji_name,
            hostname,
            image,
            object_id,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING emoji
        ",
        &[
            &emoji_id,
            &emoji_name,
            &hostname,
            &image,
            &object_id,
            &updated_at,
        ],
    ).await.map_err(catch_unique_violation("emoji"))?;
    let emoji = row.try_get("emoji")?;
    Ok(emoji)
}

pub async fn update_emoji(
    db_client: &impl GenericClient,
    emoji_id: &Uuid,
    file_name: &str,
    media_type: &str,
    updated_at: &DateTime<Utc>,
) -> Result<DbEmoji, DatabaseError> {
    let image = ProfileImage {
        file_name: file_name.to_string(),
        media_type: Some(media_type.to_string()),
    };
    let row = db_client.query_one(
        "
        UPDATE emoji
        SET
            image = $1,
            updated_at = $2
        WHERE id = $4
        RETURNING emoji
        ",
        &[
            &image,
            &updated_at,
            &emoji_id,
        ],
    ).await?;
    let emoji = row.try_get("emoji")?;
    Ok(emoji)
}

pub async fn get_emoji_by_remote_object_id(
    db_client: &impl GenericClient,
    object_id: &str,
) -> Result<DbEmoji, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT emoji
        FROM emoji WHERE object_id = $1
        ",
        &[&object_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("emoji"))?;
    let emoji = row.try_get("emoji")?;
    Ok(emoji)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_emoji() {
        let db_client = &create_test_database().await;
        let emoji_name = "test";
        let hostname = "example.org";
        let file_name = "test.png";
        let media_type = "image/png";
        let object_id = "https://example.org/emojis/test";
        let updated_at = Utc::now();
        let DbEmoji { id: emoji_id, .. } = create_emoji(
            db_client,
            emoji_name,
            Some(hostname),
            file_name,
            media_type,
            Some(object_id),
            &updated_at,
        ).await.unwrap();
        let emoji = get_emoji_by_remote_object_id(
            db_client,
            object_id,
        ).await.unwrap();
        assert_eq!(emoji.id, emoji_id);
        assert_eq!(emoji.emoji_name, emoji_name);
        assert_eq!(emoji.hostname, Some(hostname.to_string()));
    }
}
