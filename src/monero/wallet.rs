use monero_rpc::{
    HashString,
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

    #[error("{0}")]
    WalletRpcError(&'static str),

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error("other error")]
    OtherError(&'static str),
}

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#create_wallet
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

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#open_wallet
pub async fn open_monero_wallet(
    config: &MoneroConfig,
) -> Result<WalletClient, MoneroError> {
    let wallet_client = RpcClient::new(config.wallet_url.clone()).wallet();
    if let Some(ref wallet_name) = config.wallet_name {
        wallet_client.open_wallet(
            wallet_name.clone(),
            config.wallet_password.clone(),
        ).await?;
    };
    Ok(wallet_client)
}

pub async fn create_monero_address(
    config: &MoneroConfig,
) -> Result<Address, MoneroError> {
    let wallet_client = open_monero_wallet(config).await?;
    let (address, address_index) =
        wallet_client.create_address(DEFAULT_ACCOUNT, None).await?;
    log::info!("created monero address {}/{}", DEFAULT_ACCOUNT, address_index);
    Ok(address)
}

pub fn get_single_item<T: Clone>(items: Vec<T>) -> Result<T, MoneroError> {
    if let [item] = &items[..] {
        Ok(item.clone())
    } else {
        Err(MoneroError::WalletRpcError("expected single item"))
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

/// https://monerodocs.org/interacting/monero-wallet-rpc-reference/#sweep_all
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
    let HashString(tx_hash) = get_single_item(sweep_data.tx_hash_list)?;
    let amount = get_single_item(sweep_data.amount_list)?;
    let fee = get_single_item(sweep_data.fee_list)?;

    // TODO: transaction can fail
    // https://github.com/monero-project/monero/issues/8372
    let maybe_transfer = wallet_client.get_transfer(
        tx_hash,
        Some(DEFAULT_ACCOUNT),
    ).await?;
    let transfer_status = maybe_transfer
        .map(|data| data.transfer_type.into())
        .unwrap_or("dropped");
    if transfer_status == "dropped" || transfer_status == "failed" {
        log::error!(
            "sent transaction {:x} from {}/{}, {}",
            tx_hash,
            DEFAULT_ACCOUNT,
            from_address,
            transfer_status,
        );
        return Err(MoneroError::WalletRpcError("transaction failed"));
    };

    log::info!(
        "sent transaction {:x} from {}/{}, amount {}, fee {}",
        tx_hash,
        DEFAULT_ACCOUNT,
        from_address,
        amount,
        fee,
    );
    Ok(amount)
}
