use chrono::{DateTime, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::DatabaseError;
use crate::models::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use crate::utils::id::new_uuid;
use super::types::DbMediaAttachment;

pub async fn create_attachment(
    db_client: &impl GenericClient,
    owner_id: &Uuid,
    file_name: String,
    media_type: Option<String>,
) -> Result<DbMediaAttachment, DatabaseError> {
    let attachment_id = new_uuid();
    let inserted_row = db_client.query_one(
        "
        INSERT INTO media_attachment (id, owner_id, media_type, file_name)
        VALUES ($1, $2, $3, $4)
        RETURNING media_attachment
        ",
        &[&attachment_id, &owner_id, &media_type, &file_name],
    ).await?;
    let db_attachment: DbMediaAttachment = inserted_row.try_get("media_attachment")?;
    Ok(db_attachment)
}

pub async fn set_attachment_ipfs_cid(
    db_client: &impl GenericClient,
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
    db_client: &impl GenericClient,
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
