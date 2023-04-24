use serde::Serialize;

#[derive(thiserror::Error, Debug)]
#[error("canonicalization error")]
pub struct CanonicalizationError(#[from] serde_json::Error);

/// JCS: https://www.rfc-editor.org/rfc/rfc8785
pub fn canonicalize_object(object: &impl Serialize) -> Result<String, CanonicalizationError> {
    let object_str = serde_jcs::to_string(object)?;
    Ok(object_str)
}
