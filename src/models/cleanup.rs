use crate::database::{DatabaseClient, DatabaseError};

pub struct DeletionQueue {
    pub files: Vec<String>,
    pub ipfs_objects: Vec<String>,
}

pub async fn find_orphaned_files(
    db_client: &impl DatabaseClient,
    files: Vec<String>,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT DISTINCT fname
        FROM unnest($1::text[]) AS fname
        WHERE
            NOT EXISTS (
                SELECT 1 FROM media_attachment WHERE file_name = fname
            )
            AND NOT EXISTS (
                SELECT 1 FROM actor_profile
                WHERE avatar ->> 'file_name' = fname
                    OR banner ->> 'file_name' = fname
            )
            AND NOT EXISTS (
                SELECT 1 FROM emoji
                WHERE image ->> 'file_name' = fname
            )
        ",
        &[&files],
    ).await?;
    let orphaned_files = rows.iter()
        .map(|row| row.try_get("fname"))
        .collect::<Result<_, _>>()?;
    Ok(orphaned_files)
}

pub async fn find_orphaned_ipfs_objects(
    db_client: &impl DatabaseClient,
    ipfs_objects: Vec<String>,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT DISTINCT cid
        FROM unnest($1::text[]) AS cid
        WHERE
            NOT EXISTS (
                SELECT 1 FROM media_attachment WHERE ipfs_cid = cid
            )
            AND NOT EXISTS (
                SELECT 1 FROM post WHERE ipfs_cid = cid
            )
        ",
        &[&ipfs_objects],
    ).await?;
    let orphaned_ipfs_objects = rows.iter()
        .map(|row| row.try_get("cid"))
        .collect::<Result<_, _>>()?;
    Ok(orphaned_ipfs_objects)
}
