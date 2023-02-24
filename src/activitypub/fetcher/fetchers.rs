use std::path::Path;
use std::time::Duration;

use reqwest::{Client, Method, Proxy, RequestBuilder};
use serde_json::Value;

use mitra_config::Instance;
use mitra_utils::{
    files::sniff_media_type,
    urls::guess_protocol,
};

use crate::activitypub::{
    actors::types::Actor,
    constants::{AP_CONTEXT, AP_MEDIA_TYPE},
    identifiers::{local_actor_key_id, local_instance_actor_id},
    types::Object,
    vocabulary::GROUP,
};
use crate::http_signatures::create::{
    create_http_signature,
    HttpSignatureError,
};
use crate::media::{save_file, SUPPORTED_MEDIA_TYPES};
use crate::webfinger::types::{ActorAddress, JsonResourceDescriptor};

const FETCHER_CONNECTION_TIMEOUT: u64 = 30;
const FETCHER_TIMEOUT: u64 = 180;

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

    #[error("file size exceeds limit")]
    FileTooLarge,

    #[error("too many objects")]
    RecursionError,

    #[error("{0}")]
    OtherError(&'static str),
}

fn build_client(instance: &Instance) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    if let Some(ref proxy_url) = instance.proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    let timeout = Duration::from_secs(FETCHER_TIMEOUT);
    let connect_timeout = Duration::from_secs(FETCHER_CONNECTION_TIMEOUT);
    client_builder
        .timeout(timeout)
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
        let instance_actor_id = local_instance_actor_id(&instance.url());
        let instance_actor_key_id = local_actor_key_id(&instance_actor_id);
        let headers = create_http_signature(
            Method::GET,
            url,
            "",
            &instance.actor_key,
            &instance_actor_key_id,
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

pub async fn fetch_file(
    instance: &Instance,
    url: &str,
    maybe_media_type: Option<&str>,
    file_max_size: usize,
    output_dir: &Path,
) -> Result<(String, usize, Option<String>), FetchError> {
    let client = build_client(instance)?;
    let request_builder =
        build_request(instance, client, Method::GET, url);
    let response = request_builder.send().await?.error_for_status()?;
    if let Some(file_size) = response.content_length() {
        let file_size: usize = file_size.try_into()
            .expect("value should be within bounds");
        if file_size > file_max_size {
            return Err(FetchError::FileTooLarge);
        };
    };
    let file_data = response.bytes().await?;
    let file_size = file_data.len();
    if file_size > file_max_size {
        return Err(FetchError::FileTooLarge);
    };
    let maybe_media_type = maybe_media_type
        .map(|media_type| media_type.to_string())
        // Sniff media type if not provided
        .or(sniff_media_type(&file_data))
        // Remove media type if it is not supported to prevent XSS
        .filter(|media_type| {
            if SUPPORTED_MEDIA_TYPES.contains(&media_type.as_str()) {
                true
            } else {
                log::info!(
                    "unsupported media type {}: {}",
                    media_type,
                    url,
                );
                false
            }
        });
    let file_name = save_file(
        file_data.to_vec(),
        output_dir,
        maybe_media_type.as_deref(),
    )?;
    Ok((file_name, file_size, maybe_media_type))
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
    // Lemmy servers can have Group and Person actors with the same name
    // https://github.com/LemmyNet/lemmy/issues/2037
    let ap_type_property = format!("{}#type", AP_CONTEXT);
    let group_link = jrd.links.iter()
        .find(|link| {
            link.rel == "self" &&
            link.properties
                .get(&ap_type_property)
                .map(|val| val.as_str()) == Some(GROUP)
        });
    let link = if let Some(link) = group_link {
        // Prefer Group if the actor type is provided
        link
    } else {
        // Otherwise take first "self" link
        jrd.links.iter()
            .find(|link| link.rel == "self")
            .ok_or(FetchError::OtherError("self link not found"))?
    };
    let actor_url = link.href.as_ref()
        .ok_or(FetchError::OtherError("account href not found"))?
        .to_string();
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
