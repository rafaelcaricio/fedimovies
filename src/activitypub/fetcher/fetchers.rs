use std::path::Path;

use reqwest::{Client, Method, RequestBuilder};
use serde::Deserialize;
use serde_json::{Value as JsonValue};

use mitra_config::Instance;
use mitra_utils::{
    files::sniff_media_type,
    urls::guess_protocol,
};

use crate::activitypub::{
    actors::types::Actor,
    constants::{AP_CONTEXT, AP_MEDIA_TYPE},
    http_client::{build_federation_client, get_network_type},
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

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error(transparent)]
    SignatureError(#[from] HttpSignatureError),

    #[error("inavlid URL")]
    UrlError(#[from] url::ParseError),

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

fn build_client(
    instance: &Instance,
    request_url: &str,
) -> Result<Client, FetchError> {
    let network = get_network_type(request_url)?;
    let client = build_federation_client(
        instance,
        network,
        instance.fetcher_timeout,
    )?;
    Ok(client)
}

fn build_request(
    instance: &Instance,
    client: Client,
    method: Method,
    url: &str,
) -> RequestBuilder {
    let mut request_builder = client.request(method, url);
    if !instance.is_private {
        // Public instances should set User-Agent header
        request_builder = request_builder
            .header(reqwest::header::USER_AGENT, instance.agent());
    };
    request_builder
}

/// Sends GET request to fetch AP object
async fn send_request(
    instance: &Instance,
    url: &str,
) -> Result<String, FetchError> {
    let client = build_client(instance, url)?;
    let mut request_builder = build_request(instance, client, Method::GET, url)
        .header(reqwest::header::ACCEPT, AP_MEDIA_TYPE);

    if !instance.is_private {
        // Only public instances can send signed requests
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
    let client = build_client(instance, url)?;
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
    let client = build_client(instance, &webfinger_url)?;
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
    let actor_json = send_request(instance, actor_url).await?;
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
    let object_json = send_request(instance, object_url).await?;
    let object_value: JsonValue = serde_json::from_str(&object_json)?;
    let object: Object = serde_json::from_value(object_value)?;
    Ok(object)
}

pub async fn fetch_outbox(
    instance: &Instance,
    outbox_url: &str,
    limit: usize,
) -> Result<Vec<JsonValue>, FetchError> {
    #[derive(Deserialize)]
    struct Collection {
        first: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CollectionPage {
        ordered_items: Vec<JsonValue>,
    }
    let collection_json = send_request(instance, outbox_url).await?;
    let collection: Collection = serde_json::from_str(&collection_json)?;
    let page_json = send_request(instance, &collection.first).await?;
    let page: CollectionPage = serde_json::from_str(&page_json)?;
    let activities = page.ordered_items.into_iter()
        .take(limit).collect();
    Ok(activities)
}
