use std::path::Path;

use web3::{api::Web3, transports::Http};

use crate::utils::files::write_file;
use super::errors::EthereumError;

const BLOCK_NUMBER_FILE_NAME: &str = "current_block";
pub const CHAIN_REORG_MAX_DEPTH: u64 = 100;

pub fn save_current_block_number(
    storage_dir: &Path,
    block_number: u64,
) -> Result<(), EthereumError> {
    let file_path = storage_dir.join(BLOCK_NUMBER_FILE_NAME);
    write_file(block_number.to_string().as_bytes(), &file_path)
        .map_err(|_| EthereumError::OtherError("failed to save current block"))?;
    Ok(())
}

fn read_current_block_number(
    storage_dir: &Path,
) -> Result<Option<u64>, EthereumError> {
    let file_path = storage_dir.join(BLOCK_NUMBER_FILE_NAME);
    let block_number = if file_path.exists() {
        let block_number: u64 = std::fs::read_to_string(&file_path)
            .map_err(|_| EthereumError::OtherError("failed to read current block"))?
            .parse()
            .map_err(|_| EthereumError::OtherError("failed to parse block number"))?;
        Some(block_number)
    } else {
        None
    };
    Ok(block_number)
}

pub async fn get_current_block_number(
    web3: &Web3<Http>,
    storage_dir: &Path,
) -> Result<u64, EthereumError> {
    let block_number = match read_current_block_number(storage_dir)? {
        Some(block_number) => block_number,
        None => {
            // Save block number when connecting to the node for the first time
            let block_number = web3.eth().block_number().await?.as_u64();
            save_current_block_number(storage_dir, block_number)?;
            block_number
        },
    };
    Ok(block_number)
}
