use monero_rpc::RpcClient;
use monero_rpc::monero::Address;

use crate::config::MoneroConfig;

const DEFAULT_ACCOUNT: u32 = 0;

#[derive(thiserror::Error, Debug)]
pub enum MoneroError {
    #[error(transparent)]
    WalletError(#[from] anyhow::Error),
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
