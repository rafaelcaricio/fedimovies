use monero_rpc::RpcClient;

use crate::config::MoneroConfig;

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
