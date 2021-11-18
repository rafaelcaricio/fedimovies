use actix_web::http::Method;
use rsa::RsaPrivateKey;

use crate::config::{Config, Instance};
use crate::http_signatures::create::{create_http_signature, SignatureError};
use crate::models::users::types::User;
use crate::utils::crypto::deserialize_private_key;
use super::activity::Activity;
use super::actor::Actor;
use super::constants::ACTIVITY_CONTENT_TYPE;
use super::views::get_actor_url;

#[derive(thiserror::Error, Debug)]
pub enum DelivererError {
    #[error("key error")]
    KeyDeserializationError(#[from] rsa::pkcs8::Error),

    #[error(transparent)]
    SignatureError(#[from] SignatureError),

    #[error("activity serialization error")]
    SerializationError(#[from] serde_json::Error),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("http error {0:?}")]
    HttpError(reqwest::StatusCode),
}

async fn send_activity(
    instance: &Instance,
    actor_key: &RsaPrivateKey,
    actor_key_id: &str,
    activity_json: &str,
    inbox_url: &str,
) -> Result<(), DelivererError> {
    log::info!("sending activity to {}: {}", inbox_url, activity_json);
    let headers = create_http_signature(
        Method::POST,
        inbox_url,
        activity_json,
        actor_key,
        actor_key_id,
    )?;

    let client = reqwest::Client::new();
    let request = client.post(inbox_url)
        .header("Host", headers.host)
        .header("Date", headers.date)
        .header("Digest", headers.digest.unwrap())
        .header("Signature", headers.signature)
        .header("Content-Type", ACTIVITY_CONTENT_TYPE)
        .body(activity_json.to_owned());

    if instance.is_private {
        log::info!(
            "private mode: not sending activity to {}",
            inbox_url,
        );
    } else {
        // Default timeout is 30s
        let response = request.send().await?;
        let response_status = response.status();
        let response_text = response.text().await?;
        log::info!(
            "remote server response: {}",
            response_text,
        );
        if response_status.is_client_error() || response_status.is_server_error() {
            return Err(DelivererError::HttpError(response_status));
        };
    };
    Ok(())
}

async fn deliver_activity_worker(
    instance: Instance,
    sender: User,
    activity: Activity,
    recipients: Vec<Actor>,
) -> Result<(), DelivererError> {
    let actor_key = deserialize_private_key(&sender.private_key)?;
    let actor_key_id = format!(
        "{}#main-key",
        get_actor_url(
            &instance.url(),
            &sender.profile.username,
        ),
    );
    let activity_json = serde_json::to_string(&activity)?;
    let mut inboxes: Vec<String> = recipients.into_iter()
        .map(|actor| actor.inbox)
        .collect();
    inboxes.sort();
    inboxes.dedup();
    for inbox_url in inboxes {
        // TODO: retry on error
        if let Err(err) = send_activity(
            &instance,
            &actor_key,
            &actor_key_id,
            &activity_json,
            &inbox_url,
        ).await {
            log::error!("{}", err);
        }
    };
    Ok(())
}

pub fn deliver_activity(
    config: &Config,
    sender: &User,
    activity: Activity,
    recipients: Vec<Actor>,
) -> () {
    let instance = config.instance();
    let sender = sender.clone();
    actix_rt::spawn(async move {
        deliver_activity_worker(
            instance,
            sender,
            activity,
            recipients,
        ).await.unwrap_or_else(|err| {
            log::error!("{}", err);
        });
    });
}
