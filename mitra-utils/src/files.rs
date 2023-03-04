use std::fs::{
    set_permissions,
    File,
    Permissions,
};
use std::io::Error;
use std::io::prelude::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use mime_guess::get_mime_extensions_str;
use mime_sniffer::MimeTypeSniffer;

pub fn sniff_media_type(data: &[u8]) -> Option<String> {
    data.sniff_mime_type().map(|val| val.to_string())
}

pub fn get_media_type_extension(media_type: &str) -> Option<&'static str> {
    match media_type {
        // Override extension provided by mime_guess
        "image/jpeg" => Some("jpg"),
        _ => {
            get_mime_extensions_str(media_type)
                .and_then(|extensions| extensions.first())
                .copied()
        }
    }
}

pub fn write_file(data: &[u8], file_path: &Path) -> Result<(), Error> {
    let mut file = File::create(file_path)?;
    file.write_all(data)?;
    Ok(())
}

pub fn set_file_permissions(file_path: &Path, mode: u32) -> Result<(), Error> {
    let permissions = Permissions::from_mode(mode);
    set_permissions(file_path, permissions)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_media_type_extension() {
        assert_eq!(
            get_media_type_extension("image/png"),
            Some("png"),
        );
        assert_eq!(
            get_media_type_extension("image/jpeg"),
            Some("jpg"),
        );
    }
}
