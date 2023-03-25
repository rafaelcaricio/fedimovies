use std::fs::remove_file;
use std::io::Error;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use mitra_config::Config;
use mitra_utils::files::{get_media_type_extension, write_file};

use crate::ipfs::store as ipfs_store;
use crate::models::cleanup::DeletionQueue;

pub const SUPPORTED_MEDIA_TYPES: [&str; 11] = [
    "audio/mpeg",
    "audio/ogg",
    "audio/x-wav",
    "image/apng",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/webp",
    "video/mp4",
    "video/ogg",
    "video/webm",
];

/// Generates unique file name based on file contents
fn get_file_name(data: &[u8], media_type: Option<&str>) -> String {
    let digest = Sha256::digest(data);
    let mut file_name = hex::encode(digest);
    let maybe_extension = media_type
        .and_then(get_media_type_extension);
    if let Some(extension) = maybe_extension {
        // Append extension for known media types
        file_name = format!("{}.{}", file_name, extension);
    };
    file_name
}

/// Save validated file to specified directory
pub fn save_file(
    data: Vec<u8>,
    output_dir: &Path,
    media_type: Option<&str>,
) -> Result<String, Error> {
    let file_name = get_file_name(&data, media_type);
    let file_path = output_dir.join(&file_name);
    write_file(&data, &file_path)?;
    Ok(file_name)
}

pub fn get_file_url(instance_url: &str, file_name: &str) -> String {
    format!("{}/media/{}", instance_url, file_name)
}

pub fn remove_files(files: Vec<String>, from_dir: &Path) -> () {
    for file_name in files {
        let file_path = from_dir.join(file_name);
        let file_path_str = file_path.to_string_lossy();
        match remove_file(&file_path) {
            Ok(_) => log::info!("removed file {}", file_path_str),
            Err(err) => {
                log::warn!("failed to remove file {} ({})", file_path_str, err);
            },
        };
    };
}

pub async fn remove_media(
    config: &Config,
    queue: DeletionQueue,
) -> () {
    remove_files(queue.files, &config.media_dir());
    if !queue.ipfs_objects.is_empty() {
        match &config.ipfs_api_url {
            Some(ipfs_api_url) => {
                ipfs_store::remove(ipfs_api_url, queue.ipfs_objects).await
                    .unwrap_or_else(|err| log::error!("{}", err));
            },
            None => {
                log::error!(
                    "can not remove objects because IPFS API URL is not set: {:?}",
                    queue.ipfs_objects,
                );
            },
        }
    }
}

pub struct MediaStorage {
    pub media_dir: PathBuf,
    pub emoji_size_limit: usize,
}

impl From<&Config> for MediaStorage {
    fn from(config: &Config) -> Self {
        Self {
            media_dir: config.media_dir(),
            emoji_size_limit: config.limits.media.emoji_size_limit,
        }
    }
}

#[cfg(test)]
mod tests {
    use mitra_utils::files::sniff_media_type;
    use super::*;

    #[test]
    fn test_get_file_name() {
        let mut data = vec![];
        data.extend_from_slice(b"\x89PNG\x0D\x0A\x1A\x0A");
        let media_type = sniff_media_type(&data);
        let file_name = get_file_name(&data, media_type.as_deref());

        assert_eq!(
            file_name,
            "4c4b6a3be1314ab86138bef4314dde022e600960d8689a2c8f8631802d20dab6.png",
        );
    }
}
