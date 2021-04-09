use crate::config::{Environment, Config};
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
    config: &Config,
    sender: &User,
    activity: &Activity,
    inbox_url: &str,
) -> Result<(), DelivererError> {
    let activity_json = serde_json::to_string(&activity)?;
    log::info!("sending activity: {}", activity_json);
    let actor_key = deserialize_private_key(&sender.private_key)?;
    let actor_key_id = format!(
        "{}#main-key",
        get_actor_url(
            &config.instance_url(),
            &sender.profile.username,
        ),
    );
    let headers = create_http_signature(
        &inbox_url,
        &activity_json,
        actor_key,
        actor_key_id,
    )?;

    // Send
    match config.environment {
        Environment::Development => {
            log::info!(
                "development mode: not sending activity to {}",
                inbox_url,
            );
        },
        Environment::Production => {
            let client = reqwest::Client::new();
            // Default timeout is 30s
            let response = client.post(inbox_url)
                .header("Host", headers.host)
                .header("Date", headers.date)
                .header("Digest", headers.digest)
                .header("Signature", headers.signature)
                .header("Content-Type", ACTIVITY_CONTENT_TYPE)
                .body(activity_json)
                .send()
                .await?;
            let response_status = response.status();
            let response_text = response.text().await?;
            log::info!(
                "remote server response: {}",
                response_text,
            );
            if response_status.is_client_error() || response_status.is_server_error() {
                return Err(DelivererError::HttpError(response_status));
            }
        },
    };
    Ok(())
}

pub async fn deliver_activity(
    config: &Config,
    sender: &User,
    activity: Activity,
    recipients: Vec<Actor>,
) -> () {
    for actor in recipients {
        // TODO: retry on error
        if let Err(err) = send_activity(&config, &sender, &activity, &actor.inbox).await {
            log::error!("{}", err);
        }
    };
}
