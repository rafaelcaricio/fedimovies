use std::path::Path;
use std::time::Duration;

use reqwest::{Client, Method, Proxy, RequestBuilder};
use serde_json::Value;

use crate::activitypub::activity::Object;
use crate::activitypub::actors::types::Actor;
use crate::activitypub::constants::AP_MEDIA_TYPE;
use crate::config::Instance;
use crate::http_signatures::create::{
    create_http_signature,
    HttpSignatureError,
};
use crate::utils::files::{save_file, sniff_media_type};
use crate::utils::urls::guess_protocol;
use crate::webfinger::types::{ActorAddress, JsonResourceDescriptor};

const FETCHER_CONNECTION_TIMEOUT: u64 = 30;

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    SignatureError(#[from] HttpSignatureError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("json parse error: {0}")]
    JsonParseError(#[from] serde_json::Error),

    #[error(transparent)]
    FileError(#[from] std::io::Error),

    #[error("{0}")]
    OtherError(&'static str),
}

fn build_client(instance: &Instance) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    let connect_timeout = Duration::from_secs(FETCHER_CONNECTION_TIMEOUT);
    if let Some(ref proxy_url) = instance.proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    client_builder
        .connect_timeout(connect_timeout)
        .build()
}

fn build_request(
    instance: &Instance,
    client: Client,
    method: Method,
    url: &str,
) -> RequestBuilder {
    let mut request_builder = client.request(method, url);
    if !instance.is_private {
        // Public instance should set User-Agent header
        request_builder = request_builder
            .header(reqwest::header::USER_AGENT, instance.agent());
    };
    request_builder
}

/// Sends GET request to fetch AP object
async fn send_request(
    instance: &Instance,
    url: &str,
    query_params: &[(&str, &str)],
) -> Result<String, FetchError> {
    let client = build_client(instance)?;
    let mut request_builder = build_request(instance, client, Method::GET, url)
        .header(reqwest::header::ACCEPT, AP_MEDIA_TYPE);

    if !query_params.is_empty() {
        request_builder = request_builder.query(query_params);
    };
    if !instance.is_private {
        // Only public instance can send signed request
        let headers = create_http_signature(
            Method::GET,
            url,
            "",
            &instance.actor_key,
            &instance.actor_key_id(),
        )?;
        request_builder = request_builder
            .header("Host", headers.host)
            .header("Date", headers.date)
            .header("Signature", headers.signature);
    };

    let data = request_builder
        .send().await?
        .error_for_status()?
        .text().await?;
    Ok(data)
}

const FILE_MAX_SIZE: u64 = 1024 * 1024 * 20;

pub async fn fetch_file(
    instance: &Instance,
    url: &str,
    output_dir: &Path,
) -> Result<(String, Option<String>), FetchError> {
    let client = build_client(instance)?;
    let request_builder =
        build_request(instance, client, Method::GET, url);
    let response = request_builder.send().await?.error_for_status()?;
    if let Some(file_size) = response.content_length() {
        if file_size > FILE_MAX_SIZE {
            return Err(FetchError::OtherError("file is too large"));
        };
    };
    let file_data = response.bytes().await?;
    if file_data.len() > FILE_MAX_SIZE as usize {
        return Err(FetchError::OtherError("file is too large"));
    };
    let maybe_media_type = sniff_media_type(&file_data);
    let file_name = save_file(
        file_data.to_vec(),
        output_dir,
        maybe_media_type.as_deref(),
    )?;
    Ok((file_name, maybe_media_type))
}

pub async fn perform_webfinger_query(
    instance: &Instance,
    actor_address: &ActorAddress,
) -> Result<String, FetchError> {
    let webfinger_account_uri = format!("acct:{}", actor_address);
    let webfinger_url = format!(
        "{}://{}/.well-known/webfinger",
        guess_protocol(&actor_address.hostname),
        actor_address.hostname,
    );
    let client = build_client(instance)?;
    let request_builder =
        build_request(instance, client, Method::GET, &webfinger_url);
    let webfinger_data = request_builder
        .query(&[("resource", webfinger_account_uri)])
        .send().await?
        .error_for_status()?
        .text().await?;
    let jrd: JsonResourceDescriptor = serde_json::from_str(&webfinger_data)?;
    let link = jrd.links.into_iter()
        .find(|link| link.rel == "self")
        .ok_or(FetchError::OtherError("self link not found"))?;
    let actor_url = link.href
        .ok_or(FetchError::OtherError("account href not found"))?;
    Ok(actor_url)
}

pub async fn fetch_actor(
    instance: &Instance,
    actor_url: &str,
) -> Result<Actor, FetchError> {
    let actor_json = send_request(instance, actor_url, &[]).await?;
    let actor: Actor = serde_json::from_str(&actor_json)?;
    if actor.id != actor_url {
        log::warn!("redirected from {} to {}", actor_url, actor.id);
    };
    Ok(actor)
}

pub async fn fetch_object(
    instance: &Instance,
    object_url: &str,
) -> Result<Object, FetchError> {
    let object_json = send_request(instance, object_url, &[]).await?;
    let object_value: Value = serde_json::from_str(&object_json)?;
    let object: Object = serde_json::from_value(object_value)?;
    Ok(object)
}
