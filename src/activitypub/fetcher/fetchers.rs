use std::path::Path;

use reqwest::Method;
use serde_json::Value;

use crate::activitypub::activity::Object;
use crate::activitypub::actor::{Actor, ActorAddress};
use crate::activitypub::constants::ACTIVITY_CONTENT_TYPE;
use crate::config::Instance;
use crate::http_signatures::create::{create_http_signature, SignatureError};
use crate::utils::files::{save_file, FileError};
use crate::webfinger::types::JsonResourceDescriptor;

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    SignatureError(#[from] SignatureError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("json parse error")]
    JsonParseError(#[from] serde_json::Error),

    #[error("file error")]
    FileError(#[from] FileError),

    #[error("{0}")]
    OtherError(&'static str),
}

/// Sends GET request to fetch AP object
async fn send_request(
    instance: &Instance,
    url: &str,
    query_params: &[(&str, &str)],
) -> Result<String, FetchError> {
    let client = reqwest::Client::new();
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
        .header(reqwest::header::ACCEPT, ACTIVITY_CONTENT_TYPE)
        .send().await?
        .error_for_status()?
        .text().await?;
    Ok(data)
}

pub async fn fetch_file(
    url: &str,
    output_dir: &Path,
) -> Result<(String, Option<String>), FetchError> {
    let response = reqwest::get(url).await?;
    let file_data = response.bytes().await?;
    let (file_name, media_type) = save_file(file_data.to_vec(), output_dir)?;
    Ok((file_name, media_type))
}

pub async fn perform_webfinger_query(
    instance: &Instance,
    actor_address: &ActorAddress,
) -> Result<String, FetchError> {
    let webfinger_account_uri = format!("acct:{}", actor_address.to_string());
    // TOOD: support http
    let webfinger_url = format!(
        "https://{}/.well-known/webfinger",
        actor_address.instance,
    );
    let client = reqwest::Client::new();
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
    let actor = serde_json::from_str(&actor_json)?;
    Ok(actor)
}

pub async fn fetch_avatar_and_banner(
    actor: &Actor,
    media_dir: &Path,
) -> Result<(Option<String>, Option<String>), FetchError> {
    let avatar = match &actor.icon {
        Some(icon) => {
            let (file_name, _) = fetch_file(
                &icon.url,
                media_dir,
            ).await?;
            Some(file_name)
        },
        None => None,
    };
    let banner = match &actor.image {
        Some(image) => {
            let (file_name, _) = fetch_file(
                &image.url,
                media_dir,
            ).await?;
            Some(file_name)
        },
        None => None,
    };
    Ok((avatar, banner))
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
