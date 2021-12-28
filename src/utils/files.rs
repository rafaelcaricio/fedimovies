use std::fs::{remove_file, File};
use std::io::prelude::*;
use std::path::Path;

use mime_guess::get_mime_extensions_str;
use mime_sniffer::MimeTypeSniffer;
use sha2::{Digest, Sha256};

#[derive(thiserror::Error, Debug)]
pub enum FileError {
    #[error(transparent)]
    WriteError(#[from] std::io::Error),

    #[error("base64 decoding error")]
    Base64DecodingError(#[from] base64::DecodeError),

    #[error("invalid media type")]
    InvalidMediaType,
}

pub fn save_file(data: Vec<u8>, output_dir: &Path) -> Result<String, FileError> {
    let digest = Sha256::digest(&data);
    let mut file_name = hex::encode(digest);
    let maybe_extension = data.sniff_mime_type()
        .and_then(get_mime_extensions_str)
        .and_then(|extensions| extensions.first());
    if let Some(extension) = maybe_extension {
        // Append extension for known media types
        file_name = format!("{}.{}", file_name, extension);
    }
    let file_path = output_dir.join(&file_name);
    let mut file = File::create(&file_path)?;
    file.write_all(&data)?;
    Ok(file_name)
}

fn sniff_media_type(data: &[u8]) -> Option<String> {
    data.sniff_mime_type().map(|val| val.to_string())
}

pub fn save_b64_file(
    b64data: &str,
    output_dir: &Path,
) -> Result<(String, Option<String>), FileError> {
    let data = base64::decode(b64data)?;
    let media_type = sniff_media_type(&data);
    let file_name = save_file(data, output_dir)?;
    Ok((file_name, media_type))
}

pub fn save_validated_b64_file(
    b64data: &str,
    output_dir: &Path,
    media_type_prefix: &str,
) -> Result<(String, String), FileError> {
    let data = base64::decode(b64data)?;
    let media_type = sniff_media_type(&data)
        .ok_or(FileError::InvalidMediaType)?;
    if !media_type.starts_with(media_type_prefix) {
        return Err(FileError::InvalidMediaType);
    }
    let file_name = save_file(data, output_dir)?;
    Ok((file_name, media_type))
}

pub fn get_file_url(instance_url: &str, file_name: &str) -> String {
    format!("{}/media/{}", instance_url, file_name)
}

pub fn remove_files(files: Vec<String>, from_dir: &Path) -> () {
    for file_name in files {
        let file_path = from_dir.join(&file_name);
        let file_path_str = file_path.to_string_lossy();
        match remove_file(&file_path) {
            Ok(_) => log::info!("removed file {}", file_path_str),
            Err(_) => log::warn!("failed to remove file {}", file_path_str),
        }
    }
}
