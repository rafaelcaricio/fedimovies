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

pub fn get_media_type_extension(media_type: &str) -> Option<String> {
    get_mime_extensions_str(media_type)
        .and_then(|extensions| extensions.first())
        .map(|extension| extension.to_string())
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
