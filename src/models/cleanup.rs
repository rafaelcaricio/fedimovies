use tokio_postgres::GenericClient;

use crate::config::Config;
use crate::database::DatabaseError;
use crate::ipfs::store as ipfs_store;
use crate::utils::files::remove_files;

pub struct DeletionQueue {
    pub files: Vec<String>,
    pub ipfs_objects: Vec<String>,
}

impl DeletionQueue {
    pub async fn process(self, config: &Config) -> () {
        remove_files(self.files, &config.media_dir());
        if !self.ipfs_objects.is_empty() {
            match &config.ipfs_api_url {
                Some(ipfs_api_url) => {
                    ipfs_store::remove(ipfs_api_url, self.ipfs_objects).await
                        .unwrap_or_else(|err| log::error!("{}", err));
                },
                None => {
                    log::error!(
                        "can not remove objects because IPFS API URL is not set: {:?}",
                        self.ipfs_objects,
                    );
                },
            }
        }
    }
}

pub async fn find_orphaned_files(
    db_client: &impl GenericClient,
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

pub async fn find_orphaned_ipfs_objects(
    db_client: &impl GenericClient,
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
