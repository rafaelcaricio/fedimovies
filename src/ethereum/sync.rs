use std::collections::HashMap;
use std::path::{Path, PathBuf};

use web3::{api::Web3, transports::Http, types::Address};

use crate::utils::files::write_file;
use super::errors::EthereumError;

const BLOCK_NUMBER_FILE_NAME: &str = "current_block";
const CHAIN_REORG_MAX_DEPTH: u64 = 100;
const CHAIN_SYNC_STEP: u64 = 1000;

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

#[derive(Clone)]
pub struct SyncState {
    pub current_block: u64,
    contracts: HashMap<Address, u64>,

    pub storage_dir: PathBuf,
}

impl SyncState {
    pub fn new(
        current_block: u64,
        contracts: Vec<Address>,
        storage_dir: &Path,
    ) -> Self {
        let mut contract_map = HashMap::new();
        for address in contracts {
            contract_map.insert(address, current_block);
        };
        Self {
            current_block,
            contracts: contract_map,
            storage_dir: storage_dir.to_path_buf(),
        }
    }

    pub fn get_scan_range(&self, contract_address: &Address) -> (u64, u64) {
        let current_block = self.contracts[contract_address];
        // Take reorgs into account
        let safe_current_block = current_block.saturating_sub(CHAIN_REORG_MAX_DEPTH);
        (safe_current_block, safe_current_block + CHAIN_SYNC_STEP)
    }

    pub fn is_out_of_sync(&self, contract_address: &Address) -> bool {
        if let Some(max_value) = self.contracts.values().max().copied() {
            if self.contracts[contract_address] == max_value {
                return false;
            };
        };
        true
    }

    pub fn update(
        &mut self,
        contract_address: &Address,
        block_number: u64,
    ) -> bool {
        self.contracts.insert(*contract_address, block_number);
        if let Some(min_value) = self.contracts.values().min().copied() {
            if min_value > self.current_block {
                self.current_block = min_value;
                log::info!("synced to block {}", self.current_block);
                return true;
            };
        };
        false
    }
}
