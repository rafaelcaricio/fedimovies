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

pub async fn find_orphaned_files(
    db_client: &impl GenericClient,
    files: Vec<String>,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT fname
        FROM unnest($1::text[]) AS fname
        WHERE
            NOT EXISTS (
                SELECT 1 FROM media_attachment WHERE file_name = fname
            )
            AND NOT EXISTS (
                SELECT 1 FROM actor_profile
                WHERE avatar_file_name = fname OR banner_file_name = fname
            )
        ",
        &[&files],
    ).await?;
    let orphaned_files = rows.iter()
        .map(|row| row.try_get("fname"))
        .collect::<Result<_, _>>()?;
    Ok(orphaned_files)
}
