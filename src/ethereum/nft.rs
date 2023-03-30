use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use web3::{
    api::Web3,
    contract::{Contract, Options},
    ethabi::RawLog,
    transports::Http,
    types::{BlockNumber, FilterBuilder},
};

use mitra_config::EthereumConfig;
use mitra_models::{
    database::{get_database_client, DatabaseError, DbPool},
    posts::queries::{
        get_post_by_ipfs_cid,
        get_token_waitlist,
        set_post_token_id,
        set_post_token_tx_id,
    },
    properties::queries::{
        get_internal_property,
        set_internal_property,
    },
};

use crate::ipfs::utils::parse_ipfs_url;

use super::errors::EthereumError;
use super::signatures::{sign_contract_call, CallArgs, SignatureData};
use super::sync::SyncState;
use super::utils::parse_address;

const TOKEN_WAITLIST_MAP_PROPERTY_NAME: &str = "token_waitlist_map";

const TOKEN_WAIT_TIME: i64 = 10; // in minutes
const TOKEN_WAIT_RESET_TIME: i64 = 12 * 60; // in minutes

/// Finds posts awaiting tokenization
/// and looks for corresponding Mint events
pub async fn process_nft_events(
    web3: &Web3<Http>,
    contract: &Contract<Http>,
    sync_state: &mut SyncState,
    db_pool: &DbPool,
) -> Result<(), EthereumError> {
    let db_client = &**get_database_client(db_pool).await?;

    // Create/update token waitlist map
    let mut token_waitlist_map: HashMap<Uuid, DateTime<Utc>> =
        get_internal_property(db_client, TOKEN_WAITLIST_MAP_PROPERTY_NAME)
            .await?.unwrap_or_default();
    token_waitlist_map.retain(|_, waiting_since| {
        // Re-add token to waitlist if waiting for too long
        let duration = Utc::now() - *waiting_since;
        duration.num_minutes() < TOKEN_WAIT_RESET_TIME
    });
    let token_waitlist = get_token_waitlist(db_client).await?;
    for post_id in token_waitlist {
        if !token_waitlist_map.contains_key(&post_id) {
            token_waitlist_map.insert(post_id, Utc::now());
        };
    };
    let token_waitlist_active_count = token_waitlist_map.values()
        .filter(|waiting_since| {
            let duration = Utc::now() - **waiting_since;
            duration.num_minutes() < TOKEN_WAIT_TIME
        })
        .count();
    if token_waitlist_active_count > 0 {
        log::info!(
            "{} posts are waiting for confirmation of tokenization tx",
            token_waitlist_active_count,
        );
    } else if !sync_state.is_out_of_sync(&contract.address()) {
        // Don't scan blockchain if already in sync and waitlist is empty
        return Ok(());
    };

    // Search for Transfer events
    let event_abi = contract.abi().event("Transfer")?;
    let (from_block, to_block) = sync_state.get_scan_range(
        &contract.address(),
        web3.eth().block_number().await?.as_u64(),
    );
    let filter = FilterBuilder::default()
        .address(vec![contract.address()])
        .topics(Some(vec![event_abi.signature()]), None, None, None)
        .from_block(BlockNumber::Number(from_block.into()))
        .to_block(BlockNumber::Number(to_block.into()))
        .build();
    let logs = web3.eth().logs(filter).await?;
    for log in logs {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.clone().0,
        };
        let event = event_abi.parse_log(raw_log)?;
        let from_address = event.params[0].value.clone().into_address()
            .ok_or(EthereumError::ConversionError)?;
        if from_address.is_zero() {
            // Mint event found
            let token_id_u256 = event.params[2].value.clone().into_uint()
                .ok_or(EthereumError::ConversionError)?;
            let token_uri: String = contract.query(
                "tokenURI", (token_id_u256,),
                None, Options::default(), None,
            ).await?;
            let tx_id_h256 = log.transaction_hash
                .ok_or(EthereumError::ConversionError)?;
            let tx_id = hex::encode(tx_id_h256.as_bytes());
            let ipfs_cid = parse_ipfs_url(&token_uri)
                .map_err(|_| EthereumError::ConversionError)?;
            let post = match get_post_by_ipfs_cid(db_client, &ipfs_cid).await {
                Ok(post) => post,
                Err(DatabaseError::NotFound(_)) => {
                    // Post was deleted
                    continue;
                },
                Err(err) => {
                    // Unexpected error
                    log::error!("{}", err);
                    continue;
                },
            };
            if post.token_id.is_none() {
                log::info!("post {} was tokenized via {}", post.id, tx_id);
                let token_id: i32 = token_id_u256.try_into()
                    .map_err(|_| EthereumError::ConversionError)?;
                set_post_token_id(db_client, &post.id, token_id).await?;
                if post.token_tx_id.as_ref() != Some(&tx_id) {
                    log::warn!("overwriting incorrect tx id {:?}", post.token_tx_id);
                    set_post_token_tx_id(db_client, &post.id, &tx_id).await?;
                };
                token_waitlist_map.remove(&post.id);
            };
        };
    };

    set_internal_property(
        db_client,
        TOKEN_WAITLIST_MAP_PROPERTY_NAME,
        &token_waitlist_map,
    ).await?;
    sync_state.update(db_client, &contract.address(), to_block).await?;
    Ok(())
}

pub fn create_mint_signature(
    blockchain_config: &EthereumConfig,
    user_address: &str,
    token_uri: &str,
) -> Result<SignatureData, EthereumError> {
    let user_address = parse_address(user_address)?;
    let call_args: CallArgs = vec![
        Box::new(user_address),
        Box::new(token_uri.to_string()),
    ];
    let signature = sign_contract_call(
        &blockchain_config.signing_key,
        blockchain_config.ethereum_chain_id(),
        &blockchain_config.contract_address,
        "mint",
        call_args,
    )?;
    Ok(signature)
}
