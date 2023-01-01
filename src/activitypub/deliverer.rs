use std::collections::BTreeMap;
use std::time::Duration;

use actix_web::http::Method;
use reqwest::{Client, Proxy};
use rsa::RsaPrivateKey;
use serde::Serialize;
use serde_json::Value;
use tokio::time::sleep;

use crate::config::Instance;
use crate::http_signatures::create::{
    create_http_signature,
    HttpSignatureError,
};
use crate::json_signatures::create::{
    is_object_signed,
    sign_object,
    JsonSignatureError,
};
use crate::models::users::types::User;
use crate::utils::crypto_rsa::deserialize_private_key;
use super::actors::types::Actor;
use super::constants::{AP_MEDIA_TYPE, ACTOR_KEY_SUFFIX};
use super::identifiers::local_actor_id;

#[derive(thiserror::Error, Debug)]
pub enum DelivererError {
    #[error("key error")]
    KeyDeserializationError(#[from] rsa::pkcs8::Error),

    #[error(transparent)]
    HttpSignatureError(#[from] HttpSignatureError),

    #[error(transparent)]
    JsonSignatureError(#[from] JsonSignatureError),

    #[error("activity serialization error")]
    SerializationError(#[from] serde_json::Error),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("http error {0:?}")]
    HttpError(reqwest::StatusCode),
}

fn build_client(instance: &Instance) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    if let Some(ref proxy_url) = instance.proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    client_builder.build()
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

    let client = build_client(instance)?;
    let request = client.post(inbox_url)
        .header("Host", headers.host)
        .header("Date", headers.date)
        .header("Digest", headers.digest.unwrap())
        .header("Signature", headers.signature)
        .header(reqwest::header::CONTENT_TYPE, AP_MEDIA_TYPE)
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
        let response_text: String = response.text().await?
            .chars().take(30).collect();
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

// 30 secs, 5 mins, 50 mins, 8 hours
fn backoff(retry_count: u32) -> Duration {
    debug_assert!(retry_count > 0);
    Duration::from_secs(3 * 10_u64.pow(retry_count))
}

async fn deliver_activity_worker(
    instance: Instance,
    sender: User,
    activity: Value,
    inboxes: Vec<String>,
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
    let activity_signed = if is_object_signed(&activity) {
        log::warn!("activity is already signed");
        activity
    } else {
        sign_object(&activity, &actor_key, &actor_key_id)?
    };

    let activity_json = serde_json::to_string(&activity_signed)?;
    if inboxes.is_empty() {
        return Ok(());
    };
    log::info!("sending activity to {} inboxes: {}", inboxes.len(), activity_json);

    let mut queue: BTreeMap<String, bool> = BTreeMap::new();
    for inbox in inboxes {
        // is_delivered: false
        queue.insert(inbox, false);
    };
    let mut retry_count = 0;
    let max_retries = 2;

    while queue.values().any(|is_delivered| !is_delivered) &&
        retry_count <= max_retries
    {
        if retry_count > 0 {
            // Wait before next attempt
            sleep(backoff(retry_count)).await;
        };
        for (inbox_url, is_delivered) in queue.iter_mut() {
            if *is_delivered {
                continue;
            };
            if let Err(error) = send_activity(
                &instance,
                &actor_key,
                &actor_key_id,
                &activity_json,
                inbox_url,
            ).await {
                log::error!(
                    "failed to deliver activity to {} (attempt #{}): {}",
                    inbox_url,
                    retry_count + 1,
                    error,
                );
            } else {
                *is_delivered = true;
            };
        };
        retry_count += 1;
    };
    Ok(())
}

pub struct OutgoingActivity {
    instance: Instance,
    sender: User,
    pub activity: Value,
    inboxes: Vec<String>,
}

impl OutgoingActivity {
    pub fn new(
        instance: &Instance,
        sender: &User,
        activity: impl Serialize,
        recipients: Vec<Actor>,
    ) -> Self {
        let mut inboxes: Vec<String> = recipients.into_iter()
            .map(|actor| actor.inbox).collect();
        inboxes.sort();
        inboxes.dedup();
        Self {
            instance: instance.clone(),
            sender: sender.clone(),
            activity: serde_json::to_value(activity)
                .expect("activity should be serializable"),
            inboxes,
        }
    }

    pub async fn deliver(self) -> Result<(), DelivererError> {
        deliver_activity_worker(
            self.instance,
            self.sender,
            self.activity,
            self.inboxes,
        ).await
    }

    pub async fn deliver_or_log(self) -> () {
        self.deliver().await.unwrap_or_else(|err| {
            log::error!("{}", err);
        });
    }

    pub fn spawn_deliver(self) -> () {
        tokio::spawn(async move {
            self.deliver_or_log().await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff() {
        assert_eq!(backoff(1).as_secs(), 30);
        assert_eq!(backoff(2).as_secs(), 300);
    }
}
