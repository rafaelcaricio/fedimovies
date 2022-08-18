use std::path::Path;

use crate::utils::files::{save_file, sniff_media_type};

#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error(transparent)]
    WriteError(#[from] std::io::Error),

    #[error("base64 decoding error")]
    Base64DecodingError(#[from] base64::DecodeError),

    #[error("invalid media type")]
    InvalidMediaType,
}

pub fn save_b64_file(
    b64data: &str,
    output_dir: &Path,
) -> Result<(String, Option<String>), UploadError> {
    let data = base64::decode(b64data)?;
    Ok(save_file(data, output_dir, None)?)
}

pub fn save_validated_b64_file(
    b64data: &str,
    output_dir: &Path,
    media_type_prefix: &str,
) -> Result<(String, String), UploadError> {
    let data = base64::decode(b64data)?;
    let media_type = sniff_media_type(&data)
        .ok_or(UploadError::InvalidMediaType)?;
    if !media_type.starts_with(media_type_prefix) {
        return Err(UploadError::InvalidMediaType);
    };
    let (file_name, _) =
        save_file(data, output_dir, Some(media_type.clone()))?;
    Ok((file_name, media_type))
}
