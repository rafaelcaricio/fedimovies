use monero_rpc::{
    RpcClient,
    SubaddressBalanceData,
    SweepAllArgs,
    TransferPriority,
    WalletClient,
};
use monero_rpc::monero::{
    cryptonote::subaddress::Index,
    util::address::Error as AddressError,
    Address,
    Amount,
};

use crate::config::MoneroConfig;
use crate::errors::DatabaseError;

pub const DEFAULT_ACCOUNT: u32 = 0;

#[derive(thiserror::Error, Debug)]
pub enum MoneroError {
    #[error(transparent)]
    WalletError(#[from] anyhow::Error),

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("other error")]
    OtherError(&'static str),
}

/// http://monerotoruzizulg5ttgat2emf4d6fbmiea25detrmmy7erypseyteyd.onion/resources/developer-guides/wallet-rpc.html#create_wallet
pub async fn create_monero_wallet(
    config: &MoneroConfig,
    name: String,
    password: Option<String>,
) -> Result<(), MoneroError> {
    let wallet_client = RpcClient::new(config.wallet_url.clone()).wallet();
    let language = "English".to_string();
    wallet_client.create_wallet(name, password, language).await?;
    Ok(())
}

pub async fn create_monero_address(
    config: &MoneroConfig,
) -> Result<Address, MoneroError> {
    let wallet_client = RpcClient::new(config.wallet_url.clone()).wallet();
    wallet_client.open_wallet(
        config.wallet_name.clone(),
        config.wallet_password.clone(),
    ).await?;
    let (address, address_index) =
        wallet_client.create_address(DEFAULT_ACCOUNT, None).await?;
    log::info!("created monero address {}/{}", DEFAULT_ACCOUNT, address_index);
    Ok(address)
}

fn get_single_item<T: Clone>(items: Vec<T>) -> Result<T, MoneroError> {
    if let [item] = &items[..] {
        Ok(item.clone())
    } else {
        Err(MoneroError::OtherError("invalid response from wallet"))
    }
}

pub async fn get_subaddress_balance(
    wallet_client: &WalletClient,
    subaddress_index: &Index,
) -> Result<SubaddressBalanceData, MoneroError> {
    let balance_data = wallet_client.get_balance(
        subaddress_index.major,
        Some(vec![subaddress_index.minor]),
    ).await?;
    let subaddress_data = get_single_item(balance_data.per_subaddress)?;
    Ok(subaddress_data)
}

/// http://monerotoruzizulg5ttgat2emf4d6fbmiea25detrmmy7erypseyteyd.onion/resources/developer-guides/wallet-rpc.html#sweep_all
pub async fn send_monero(
    wallet_client: &WalletClient,
    from_address: u32,
    to_address: Address,
) -> Result<Amount, MoneroError> {
    let sweep_args = SweepAllArgs {
        address: to_address,
        account_index: DEFAULT_ACCOUNT,
        subaddr_indices: Some(vec![from_address]),
        priority: TransferPriority::Default,
        mixin: 15,
        ring_size: 16,
        unlock_time: 1,
        get_tx_keys: None,
        below_amount: None,
        do_not_relay: None,
        get_tx_hex: None,
        get_tx_metadata: None,
    };
    let sweep_data = wallet_client.sweep_all(sweep_args).await?;
    let tx_hash = get_single_item(sweep_data.tx_hash_list)?;
    let amount = get_single_item(sweep_data.amount_list)?;
    let fee = get_single_item(sweep_data.fee_list)?;
    log::info!(
        "sent transaction {}, amount {}, fee {}",
        tx_hash,
        amount,
        fee,
    );
    Ok(amount)
}
