use std::path::PathBuf;

use serde_json::Value;

use crate::models::profiles::types::ProfileCreateData;
use crate::utils::files::{save_file, FileError};
use crate::webfinger::types::JsonResourceDescriptor;
use super::activity::Object;
use super::actor::Actor;
use super::constants::ACTIVITY_CONTENT_TYPE;

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("invalid URL")]
    UrlError(#[from] url::ParseError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("json parse error")]
    JsonParseError(#[from] serde_json::Error),

    #[error("file error")]
    FileError(#[from] FileError),

    #[error("{0}")]
    OtherError(&'static str),
}

pub async fn fetch_avatar_and_banner(
    actor: &Actor,
    media_dir: &PathBuf,
) -> Result<(Option<String>, Option<String>), FetchError> {
    let avatar = match &actor.icon {
        Some(icon) => {
            let file_name = fetch_attachment(
                &icon.url,
                media_dir,
            ).await?;
            Some(file_name)
        },
        None => None,
    };
    let banner = match &actor.image {
        Some(image) => {
            let file_name = fetch_attachment(
                &image.url,
                media_dir,
            ).await?;
            Some(file_name)
        },
        None => None,
    };
    Ok((avatar, banner))
}

pub async fn fetch_profile(
    username: &str,
    instance_uri: &str,
    media_dir: &PathBuf,
) -> Result<ProfileCreateData, FetchError> {
    let actor_address = format!("{}@{}", &username, &instance_uri);
    let webfinger_account_uri = format!("acct:{}", actor_address);
    // TOOD: support http
    let webfinger_url = format!("https://{}/.well-known/webfinger", instance_uri);
    let client = reqwest::Client::new();
    let webfinger_data = client.get(&webfinger_url)
        .query(&[("resource", webfinger_account_uri)])
        .send().await?
        .text().await?;
    let jrd: JsonResourceDescriptor = serde_json::from_str(&webfinger_data)?;
    let link = jrd.links.iter()
        .find(|link| link.rel == "self")
        .ok_or(FetchError::OtherError("self link not found"))?;
    let actor_url = link.href.as_ref()
        .ok_or(FetchError::OtherError("account href not found"))?;
    fetch_profile_by_actor_id(actor_url, media_dir).await
}

pub async fn fetch_profile_by_actor_id(
    actor_url: &str,
    media_dir: &PathBuf,
) -> Result<ProfileCreateData, FetchError> {
    let actor_host = url::Url::parse(actor_url)?
        .host_str()
        .ok_or(FetchError::OtherError("invalid URL"))?
        .to_owned();
    let client = reqwest::Client::new();
    let actor_json = client.get(actor_url)
        .header(reqwest::header::ACCEPT, ACTIVITY_CONTENT_TYPE)
        .send().await?
        .text().await?;
    let actor_value: Value = serde_json::from_str(&actor_json)?;
    let actor: Actor = serde_json::from_value(actor_value.clone())?;
    let (avatar, banner) = fetch_avatar_and_banner(&actor, media_dir).await?;
    let extra_fields = actor.extra_fields();
    let actor_address = format!(
        "{}@{}",
        actor.preferred_username,
        actor_host,
    );
    let profile_data = ProfileCreateData {
        username: actor.preferred_username,
        display_name: Some(actor.name),
        acct: actor_address,
        bio: actor.summary,
        avatar,
        banner,
        extra_fields,
        actor: Some(actor_value),
    };
    Ok(profile_data)
}

pub async fn fetch_attachment(
    url: &str,
    output_dir: &PathBuf,
) -> Result<String, FetchError> {
    let response = reqwest::get(url).await?;
    let file_data = response.bytes().await?;
    let file_name = save_file(file_data.to_vec(), output_dir)?;
    Ok(file_name)
}

pub async fn fetch_object(
    object_url: &str,
) -> Result<Object, FetchError> {
    let client = reqwest::Client::new();
    let object_json = client.get(object_url)
        .header(reqwest::header::ACCEPT, ACTIVITY_CONTENT_TYPE)
        .send().await?
        .text().await?;
    let object_value: Value = serde_json::from_str(&object_json)?;
    let object: Object = serde_json::from_value(object_value)?;
    Ok(object)
}
