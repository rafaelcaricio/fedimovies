use std::fs;
use std::path::Path;

pub const COLLECTIBLE: &str = "Collectible";
pub const MANAGER: &str = "Manager";

#[derive(thiserror::Error, Debug)]
pub enum ArtifactError {
    #[error("io error")]
    IoError(#[from] std::io::Error),
    
    #[error("json error")]
    JsonError(#[from] serde_json::Error),

    #[error("key error")]
    KeyError,
}

pub fn load_abi(
    contract_dir: &Path,
    contract_name: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let contract_artifact_path = contract_dir.join(format!("{}.json", contract_name));
    let contract_artifact = fs::read_to_string(contract_artifact_path)?;
    let contract_artifact_value: serde_json::Value = serde_json::from_str(&contract_artifact)?;
    let contract_abi = contract_artifact_value.get("abi")
        .ok_or(ArtifactError::KeyError)?
        .to_string().as_bytes().to_vec();
    Ok(contract_abi)
}
