use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::database::{DatabaseClient, DatabaseError};
use crate::models::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use super::types::DbMediaAttachment;

pub async fn create_attachment(
    db_client: &impl DatabaseClient,
    owner_id: &Uuid,
    file_name: String,
    file_size: usize,
    media_type: Option<String>,
) -> Result<DbMediaAttachment, DatabaseError> {
    let attachment_id = generate_ulid();
    let file_size: i32 = file_size.try_into()
        .expect("value should be within bounds");
    let inserted_row = db_client.query_one(
        "
        INSERT INTO media_attachment (
            id,
            owner_id,
            file_name,
            file_size,
            media_type
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING media_attachment
        ",
        &[
            &attachment_id,
            &owner_id,
            &file_name,
            &file_size,
            &media_type,
        ],
    ).await?;
    let db_attachment: DbMediaAttachment = inserted_row.try_get("media_attachment")?;
    Ok(db_attachment)
}

pub async fn set_attachment_ipfs_cid(
    db_client: &impl DatabaseClient,
    attachment_id: &Uuid,
    ipfs_cid: &str,
) -> Result<DbMediaAttachment, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE media_attachment
        SET ipfs_cid = $1
        WHERE id = $2 AND ipfs_cid IS NULL
        RETURNING media_attachment
        ",
        &[&ipfs_cid, &attachment_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("attachment"))?;
    let db_attachment = row.try_get("media_attachment")?;
    Ok(db_attachment)
}

pub async fn delete_unused_attachments(
    db_client: &impl DatabaseClient,
    created_before: &DateTime<Utc>,
) -> Result<DeletionQueue, DatabaseError> {
    let rows = db_client.query(
        "
        DELETE FROM media_attachment
        WHERE post_id IS NULL AND created_at < $1
        RETURNING file_name, ipfs_cid
        ",
        &[&created_before],
    ).await?;
    let mut files = vec![];
    let mut ipfs_objects = vec![];
    for row in rows {
        let file_name = row.try_get("file_name")?;
        files.push(file_name);
        if let Some(ipfs_cid) = row.try_get("ipfs_cid")? {
            ipfs_objects.push(ipfs_cid);
        };
    };
    let orphaned_files = find_orphaned_files(db_client, files).await?;
    let orphaned_ipfs_objects = find_orphaned_ipfs_objects(db_client, ipfs_objects).await?;
    Ok(DeletionQueue {
        files: orphaned_files,
        ipfs_objects: orphaned_ipfs_objects,
    })
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::{
        profiles::types::ProfileCreateData,
        profiles::queries::create_profile,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_attachment() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let file_name = "test.jpg";
        let file_size = 10000;
        let media_type = "image/png";
        let attachment = create_attachment(
            db_client,
            &profile.id,
            file_name.to_string(),
            file_size,
            Some(media_type.to_string()),
        ).await.unwrap();
        assert_eq!(attachment.owner_id, profile.id);
        assert_eq!(attachment.file_name, file_name);
        assert_eq!(attachment.file_size.unwrap(), file_size as i32);
        assert_eq!(attachment.media_type.unwrap(), media_type);
        assert_eq!(attachment.post_id.is_none(), true);
    }
}
