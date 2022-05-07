use std::fs::{
    remove_file,
    set_permissions,
    File,
    Permissions,
};
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;
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

fn sniff_media_type(data: &[u8]) -> Option<String> {
    data.sniff_mime_type().map(|val| val.to_string())
}

/// Generates unique file name based on file contents
fn get_file_name(data: &[u8], media_type: Option<&str>) -> String {
    let digest = Sha256::digest(data);
    let mut file_name = hex::encode(digest);
    let maybe_extension = media_type
        .and_then(get_mime_extensions_str)
        .and_then(|extensions| extensions.first());
    if let Some(extension) = maybe_extension {
        // Append extension for known media types
        file_name = format!("{}.{}", file_name, extension);
    };
    file_name
}

pub fn write_file(data: &[u8], file_path: &Path) -> Result<(), FileError> {
    let mut file = File::create(file_path)?;
    file.write_all(data)?;
    Ok(())
}

pub fn set_file_permissions(file_path: &Path, mode: u32) -> Result<(), FileError> {
    let permissions = Permissions::from_mode(mode);
    set_permissions(file_path, permissions)?;
    Ok(())
}

pub fn save_file(
    data: Vec<u8>,
    output_dir: &Path,
) -> Result<(String, Option<String>), FileError> {
    let media_type = sniff_media_type(&data);
    let file_name = get_file_name(&data, media_type.as_deref());
    let file_path = output_dir.join(&file_name);
    write_file(&data, &file_path)?;
    Ok((file_name, media_type))
}

pub fn save_b64_file(
    b64data: &str,
    output_dir: &Path,
) -> Result<(String, Option<String>), FileError> {
    let data = base64::decode(b64data)?;
    let media_type = sniff_media_type(&data);
    let file_name = get_file_name(&data, media_type.as_deref());
    let file_path = output_dir.join(&file_name);
    write_file(&data, &file_path)?;
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
    let file_name = get_file_name(&data, Some(&media_type));
    let file_path = output_dir.join(&file_name);
    write_file(&data, &file_path)?;
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
            Err(err) => {
                log::warn!("failed to remove file {} ({})", file_path_str, err);
            },
        };
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_file_name() {
        let mut data = vec![];
        data.extend_from_slice(b"\x89PNG\x0D\x0A\x1A\x0A");
        let media_type = data.sniff_mime_type();
        let file_name = get_file_name(&data, media_type);

        assert_eq!(
            file_name,
            "4c4b6a3be1314ab86138bef4314dde022e600960d8689a2c8f8631802d20dab6.png",
        );
    }
}
