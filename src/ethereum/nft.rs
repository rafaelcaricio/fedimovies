use std::collections::HashMap;
use std::convert::TryInto;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use web3::{
    api::Web3,
    contract::{Contract, Options},
    ethabi::{Event, EventParam, ParamType, RawLog, token::Token, encode},
    transports::Http,
    types::{BlockNumber, FilterBuilder, H256, U256},
};

use crate::config::{Config, EthereumContract};
use crate::database::{Pool, get_database_client};
use crate::errors::DatabaseError;
use crate::ipfs::utils::parse_ipfs_url;
use crate::models::posts::queries::{
    get_post_by_ipfs_cid,
    update_post,
    get_token_waitlist,
};
use super::api::connect;
use super::contracts::{MANAGER, COLLECTIBLE, load_abi};
use super::errors::EthereumError;
use super::utils::{parse_address, sign_message, SignatureData};

const TOKEN_WAIT_TIME: i64 = 10; // in minutes

pub async fn get_nft_contract(
    config: &Config,
) -> Result<(Web3<Http>, Contract<Http>), EthereumError> {
    let contract_dir = config.ethereum_contract_dir.as_ref()
        .ok_or(EthereumError::ImproperlyConfigured)?;
    let json_rpc_url = config.ethereum_json_rpc_url.as_ref()
        .ok_or(EthereumError::ImproperlyConfigured)?;
    let web3 = connect(json_rpc_url)?;
    let ethereum_config = config.ethereum_contract.as_ref()
        .ok_or(EthereumError::ImproperlyConfigured)?;

    let manager_abi = load_abi(contract_dir, MANAGER)?;
    let manager_address = parse_address(&ethereum_config.address)?;
    let manager = Contract::from_json(
        web3.eth(),
        manager_address,
        &manager_abi,
    )?;

    let token_address = manager.query(
        "collectible",
        (), None, Options::default(), None,
    ).await?;
    let token_abi = load_abi(contract_dir, COLLECTIBLE)?;
    let token = Contract::from_json(
        web3.eth(),
        token_address,
        &token_abi,
    )?;
    log::info!("NFT contract address is {:?}", token.address());
    Ok((web3, token))
}

#[derive(Debug)]
struct TokenTransfer {
    tx_id: Option<H256>,
    from: Token,
    to: Token,
    token_id: Token,
}

/// Finds posts awaiting tokenization
/// and looks for corresponding Mint events
pub async fn process_events(
    web3: &Web3<Http>,
    contract: &Contract<Http>,
    db_pool: &Pool,
    token_waitlist_map: &mut HashMap<Uuid, DateTime<Utc>>,
) -> Result<(), EthereumError> {
    let db_client = &**get_database_client(db_pool).await?;

    // Create/update token waitlist map
    let token_waitlist = get_token_waitlist(db_client).await?;
    for post_id in token_waitlist {
        if !token_waitlist_map.contains_key(&post_id) {
            token_waitlist_map.insert(post_id, Utc::now());
        }
    }
    let token_waitlist_active_count = token_waitlist_map.values()
        .filter(|waiting_since| {
            let duration = Utc::now() - **waiting_since;
            duration.num_minutes() < TOKEN_WAIT_TIME
        })
        .count();
    if token_waitlist_active_count == 0 {
        return Ok(())
    }
    log::info!(
        "{} posts are waiting for confirmation of tokenization tx",
        token_waitlist_active_count,
    );

    // Search for Transfer events
    let event_abi_params = vec![
        EventParam {
            name: "from".to_string(),
            kind: ParamType::Address,
            indexed: true,
        },
        EventParam {
            name: "to".to_string(),
            kind: ParamType::Address,
            indexed: true,
        },
        EventParam {
            name: "tokenId".to_string(),
            kind: ParamType::Uint(256),
            indexed: true,
        },
    ];
    let event_abi = Event {
        name: "Transfer".to_string(),
        inputs: event_abi_params,
        anonymous: false,
    };
    let filter = FilterBuilder::default()
        .address(vec![contract.address()])
        .topics(Some(vec![event_abi.signature()]), None, None, None)
        .from_block(BlockNumber::Earliest)
        .build();
    let logs = web3.eth().logs(filter).await?;

    // Convert web3 logs into ethabi logs
    let transfers: Vec<TokenTransfer> = logs.iter().map(|log| {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.clone().0,
        };
        match event_abi.parse_log(raw_log) {
            Ok(event) => {
                let params = event.params;
                let transfer = TokenTransfer {
                    tx_id: log.transaction_hash,
                    from: params[0].value.clone(),
                    to: params[1].value.clone(),
                    token_id: params[2].value.clone(),
                };
                Ok(transfer)
            },
            Err(err) => Err(err),
        }
    }).collect::<Result<_, web3::ethabi::Error>>()?;
    for transfer in transfers {
        let from_address = transfer.from.into_address()
            .ok_or(EthereumError::ConversionError)?;
        if from_address.is_zero() {
            // Mint event found
            let token_id_u256 = transfer.token_id.into_uint()
                .ok_or(EthereumError::ConversionError)?;
            let token_uri: String = contract.query(
                "tokenURI", (token_id_u256,),
                None, Options::default(), None,
            ).await?;
            let tx_id_h256 = transfer.tx_id.ok_or(EthereumError::ConversionError)?;
            let tx_id = hex::encode(tx_id_h256.as_bytes());
            let ipfs_cid = parse_ipfs_url(&token_uri)
                .map_err(|_| EthereumError::TokenUriParsingError)?;
            let mut post = match get_post_by_ipfs_cid(db_client, &ipfs_cid).await {
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
                post.token_id = Some(token_id);
                post.token_tx_id = Some(tx_id);
                update_post(db_client, &post).await?;
                token_waitlist_map.remove(&post.id);
            };
        };
    };
    Ok(())
}

pub fn create_mint_signature(
    contract_config: &EthereumContract,
    user_address: &str,
    token_uri: &str,
) -> Result<SignatureData, EthereumError> {
    let contract_address = parse_address(&contract_config.address)?;
    let user_address = parse_address(user_address)?;
    let chain_id: U256 = contract_config.chain_id.into();
    let chain_id_token = Token::Uint(chain_id);
    let chain_id_bin = encode(&[chain_id_token]);
    let message = [
        &chain_id_bin,
        contract_address.as_bytes(),
        "mint".as_bytes(),
        user_address.as_bytes(),
        token_uri.as_bytes(),
    ].concat();
    let signature = sign_message(&contract_config.signing_key, &message)?;
    Ok(signature)
}
