use std::cmp::max;
use std::time::Duration;

use reqwest::{Client, Proxy};

use mitra_config::Instance;

const CONNECTION_TIMEOUT: u64 = 30;

pub fn build_federation_client(
    instance: &Instance,
    is_onion: bool,
    timeout: u64,
) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    let mut maybe_proxy_url = instance.proxy_url.as_ref();
    if is_onion {
        maybe_proxy_url = maybe_proxy_url
            .or(instance.onion_proxy_url.as_ref());
    };
    if let Some(proxy_url) = maybe_proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    let request_timeout = Duration::from_secs(timeout);
    let connect_timeout = Duration::from_secs(max(
        timeout,
        CONNECTION_TIMEOUT,
    ));
    client_builder
        .timeout(request_timeout)
        .connect_timeout(connect_timeout)
        .build()
}
