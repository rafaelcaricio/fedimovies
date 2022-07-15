use actix_web::http::Method;
use rsa::RsaPrivateKey;

use crate::config::Instance;
use crate::http_signatures::create::{create_http_signature, SignatureError};
use crate::models::users::types::User;
use crate::utils::crypto::deserialize_private_key;
use super::activity::Activity;
use super::actor::Actor;
use super::constants::{ACTIVITY_CONTENT_TYPE, ACTOR_KEY_SUFFIX};
use super::identifiers::local_actor_id;

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
        .header(reqwest::header::CONTENT_TYPE, ACTIVITY_CONTENT_TYPE)
        .header(reqwest::header::USER_AGENT, instance.agent())
        .body(activity_json.to_owned());

    if instance.is_private {
        log::info!(
            "private mode: not sending activity to {}",
            inbox_url,
        );
    } else {
        let response = request.send().await?;
        let response_status = response.status();
        let response_text = response.text().await?;
        log::info!(
            "response from {}: {}",
            inbox_url,
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
        "{}{}",
        local_actor_id(
            &instance.url(),
            &sender.profile.username,
        ),
        ACTOR_KEY_SUFFIX,
    );
    let activity_json = serde_json::to_string(&activity)?;
    if recipients.is_empty() {
        return Ok(());
    };
    let mut inboxes: Vec<String> = recipients.into_iter()
        .map(|actor| actor.inbox)
        .collect();
    inboxes.sort();
    inboxes.dedup();
    log::info!("sending activity to {} inboxes: {}", inboxes.len(), activity_json);
    for inbox_url in inboxes {
        // TODO: retry on error
        if let Err(err) = send_activity(
            &instance,
            &actor_key,
            &actor_key_id,
            &activity_json,
            &inbox_url,
        ).await {
            log::error!("failed to deliver activity to {}: {}", inbox_url, err);
        };
    };
    Ok(())
}

pub struct OutgoingActivity {
    pub instance: Instance,
    pub sender: User,
    pub activity: Activity,
    pub recipients: Vec<Actor>,
}

impl OutgoingActivity {
    pub async fn deliver(self) -> Result<(), DelivererError> {
        deliver_activity_worker(
            self.instance,
            self.sender,
            self.activity,
            self.recipients,
        ).await
    }

    pub fn spawn_deliver(self) -> () {
        actix_rt::spawn(async move {
            self.deliver().await.unwrap_or_else(|err| {
                log::error!("{}", err);
            });
        });
    }
}
