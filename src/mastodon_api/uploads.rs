use std::path::Path;

use crate::errors::HttpError;
use crate::utils::files::{
    save_file,
    sniff_media_type,
    SUPPORTED_MEDIA_TYPES,
};

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
    maybe_media_type: Option<String>,
    output_dir: &Path,
    maybe_expected_prefix: Option<&str>,
) -> Result<(String, String), UploadError> {
    let data = base64::decode(b64data)?;
    if data.len() > UPLOAD_MAX_SIZE {
        return Err(UploadError::TooLarge);
    };
    // Sniff media type if not provided
    let media_type = maybe_media_type.or(sniff_media_type(&data))
        .ok_or(UploadError::InvalidMediaType)?;
    if !SUPPORTED_MEDIA_TYPES.contains(&media_type.as_str()) {
        return Err(UploadError::InvalidMediaType);
    };
    if let Some(expected_prefix) = maybe_expected_prefix {
        if !media_type.starts_with(expected_prefix) {
            return Err(UploadError::InvalidMediaType);
        };
    };
    let file_name = save_file(
        data,
        output_dir,
        Some(&media_type),
    )?;
    Ok((file_name, media_type))
}
