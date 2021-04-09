use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use super::types::DbMediaAttachment;

pub async fn create_attachment(
    db_client: &impl GenericClient,
    owner_id: &Uuid,
    media_type: Option<String>,
    file_name: String,
) -> Result<DbMediaAttachment, DatabaseError> {
    let attachment_id = Uuid::new_v4();
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
