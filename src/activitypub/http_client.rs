use std::cmp::max;
use std::time::Duration;

use reqwest::{Client, Proxy};

use fedimovies_config::Instance;
use fedimovies_utils::urls::get_hostname;

const CONNECTION_TIMEOUT: u64 = 30;

pub enum Network {
    Default,
    Tor,
    I2p,
}

pub fn get_network_type(request_url: &str) -> Result<Network, url::ParseError> {
    let hostname = get_hostname(request_url)?;
    let network = if hostname.ends_with(".onion") {
        Network::Tor
    } else if hostname.ends_with(".i2p") {
        Network::I2p
    } else {
        Network::Default
    };
    Ok(network)
}

pub fn build_federation_client(
    instance: &Instance,
    network: Network,
    timeout: u64,
) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    let mut maybe_proxy_url = instance.proxy_url.as_ref();
    match network {
        Network::Default => (),
        Network::Tor => {
            maybe_proxy_url = instance.onion_proxy_url.as_ref().or(maybe_proxy_url);
        }
        Network::I2p => {
            maybe_proxy_url = instance.i2p_proxy_url.as_ref().or(maybe_proxy_url);
        }
    };
    if let Some(proxy_url) = maybe_proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    let request_timeout = Duration::from_secs(timeout);
    let connect_timeout = Duration::from_secs(max(timeout, CONNECTION_TIMEOUT));
    client_builder
        .timeout(request_timeout)
        .connect_timeout(connect_timeout)
        .build()
}
