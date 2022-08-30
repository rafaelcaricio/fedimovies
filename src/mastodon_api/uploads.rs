use std::path::Path;

use crate::errors::HttpError;
use crate::utils::files::{save_file, sniff_media_type};

pub const UPLOAD_MAX_SIZE: usize = 1024 * 1024 * 5;

#[derive(thiserror::Error, Debug)]
pub enum UploadError {
    #[error(transparent)]
    WriteError(#[from] std::io::Error),

    #[error("base64 decoding error")]
    Base64DecodingError(#[from] base64::DecodeError),

    #[error("file is too large")]
    TooLarge,

    #[error("invalid media type")]
    InvalidMediaType,
}

impl From<UploadError> for HttpError {
    fn from(error: UploadError) -> Self {
        match error {
            UploadError::WriteError(_) => HttpError::InternalError,
            other_error => {
                HttpError::ValidationError(other_error.to_string())
            },
        }
    }
}

pub fn save_b64_file(
    b64data: &str,
    output_dir: &Path,
) -> Result<(String, Option<String>), UploadError> {
    let data = base64::decode(b64data)?;
    if data.len() > UPLOAD_MAX_SIZE {
        return Err(UploadError::TooLarge);
    };
    Ok(save_file(data, output_dir, None)?)
}

pub fn save_validated_b64_file(
    b64data: &str,
    output_dir: &Path,
    media_type_prefix: &str,
) -> Result<(String, String), UploadError> {
    let data = base64::decode(b64data)?;
    if data.len() > UPLOAD_MAX_SIZE {
        return Err(UploadError::TooLarge);
    };
    let media_type = sniff_media_type(&data)
        .ok_or(UploadError::InvalidMediaType)?;
    if !media_type.starts_with(media_type_prefix) {
        return Err(UploadError::InvalidMediaType);
    };
    let (file_name, _) =
        save_file(data, output_dir, Some(media_type.clone()))?;
    Ok((file_name, media_type))
}