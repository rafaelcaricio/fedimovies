use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::utils::id::new_uuid;
use super::types::DbMediaAttachment;

pub async fn create_attachment(
    db_client: &impl GenericClient,
    owner_id: &Uuid,
    media_type: Option<String>,
    file_name: String,
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
        WHERE id = $2
        RETURNING media_attachment
        ",
        &[&ipfs_cid, &attachment_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("attachment"))?;
    let db_attachment = row.try_get("media_attachment")?;
    Ok(db_attachment)
}
