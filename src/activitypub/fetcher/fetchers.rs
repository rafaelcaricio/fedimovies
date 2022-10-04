use std::path::Path;
use std::time::Duration;

use reqwest::{Client, Method};
use serde_json::Value;

use crate::activitypub::activity::Object;
use crate::activitypub::actors::types::{Actor, ActorAddress};
use crate::activitypub::constants::AP_MEDIA_TYPE;
use crate::config::Instance;
use crate::http_signatures::create::{create_http_signature, SignatureError};
use crate::utils::files::save_file;
use crate::webfinger::types::JsonResourceDescriptor;

const FETCHER_CONNECTION_TIMEOUT: u64 = 30;

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    SignatureError(#[from] SignatureError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("json parse error")]
    JsonParseError(#[from] serde_json::Error),

    #[error(transparent)]
    FileError(#[from] std::io::Error),

    #[error("{0}")]
    OtherError(&'static str),
}

fn build_client() -> reqwest::Result<Client> {
    let connect_timeout = Duration::from_secs(FETCHER_CONNECTION_TIMEOUT);
    Client::builder()
        .connect_timeout(connect_timeout)
        .build()
}

/// Sends GET request to fetch AP object
async fn send_request(
    instance: &Instance,
    url: &str,
    query_params: &[(&str, &str)],
) -> Result<String, FetchError> {
    let client = build_client()?;
    let mut request_builder = client.get(url);
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
    if !instance.is_private {
        // Public instance should set User-Agent header
        request_builder = request_builder
            .header(reqwest::header::USER_AGENT, instance.agent());
    };

    let data = request_builder
        .header(reqwest::header::ACCEPT, AP_MEDIA_TYPE)
        .send().await?
        .error_for_status()?
        .text().await?;
    Ok(data)
}

const FILE_MAX_SIZE: u64 = 1024 * 1024 * 20;

pub async fn fetch_file(
    url: &str,
    output_dir: &Path,
) -> Result<(String, Option<String>), FetchError> {
    let client = build_client()?;
    let response = client.get(url).send().await?;
    if let Some(file_size) = response.content_length() {
        if file_size > FILE_MAX_SIZE {
            return Err(FetchError::OtherError("file is too large"));
        };
    };
    let file_data = response.bytes().await?;
    if file_data.len() > FILE_MAX_SIZE as usize {
        return Err(FetchError::OtherError("file is too large"));
    };
    let (file_name, media_type) = save_file(file_data.to_vec(), output_dir, None)?;
    Ok((file_name, media_type))
}

pub async fn perform_webfinger_query(
    instance: &Instance,
    actor_address: &ActorAddress,
) -> Result<String, FetchError> {
    let webfinger_account_uri = format!("acct:{}", actor_address);
    // TOOD: support http
    let webfinger_url = format!(
        "https://{}/.well-known/webfinger",
        actor_address.instance,
    );
    let client = build_client()?;
    let mut request_builder = client.get(&webfinger_url);
    if !instance.is_private {
        // Public instance should set User-Agent header
        request_builder = request_builder
            .header(reqwest::header::USER_AGENT, instance.agent());
    };
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

pub async fn fetch_actor_avatar(
    actor: &Actor,
    media_dir: &Path,
    default: Option<String>,
) -> Option<String> {
    match &actor.icon {
        Some(icon) => {
            match fetch_file(&icon.url, media_dir).await {
                Ok((file_name, _)) => Some(file_name),
                Err(error) => {
                    log::warn!(
                        "failed to fetch avatar for {} ({})",
                        actor.id,
                        error,
                    );
                    default
                },
            }
        },
        None => None,
    }
}

pub async fn fetch_actor_banner(
    actor: &Actor,
    media_dir: &Path,
    default: Option<String>,
) -> Option<String> {
    match &actor.image {
        Some(image) => {
            match fetch_file(&image.url, media_dir).await {
                Ok((file_name, _)) => Some(file_name),
                Err(error) => {
                    log::warn!(
                        "failed to fetch banner for {} ({})",
                        actor.id,
                        error,
                    );
                    default
                },
            }
        },
        None => None,
    }
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
