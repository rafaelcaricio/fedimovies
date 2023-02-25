use std::collections::BTreeMap;
use std::time::Duration;

use actix_web::http::Method;
use reqwest::{Client, Proxy};
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;

use mitra_config::Instance;
use mitra_utils::crypto_rsa::deserialize_private_key;

use crate::database::{
    get_database_client,
    DatabaseClient,
    DatabaseError,
    DbPool,
};
use crate::http_signatures::create::{
    create_http_signature,
    HttpSignatureError,
};
use crate::json_signatures::create::{
    is_object_signed,
    sign_object,
    JsonSignatureError,
};
use crate::models::{
    profiles::queries::set_reachability_status,
    users::types::User,
};
use super::actors::types::Actor;
use super::constants::AP_MEDIA_TYPE;
use super::identifiers::{local_actor_id, local_actor_key_id};
use super::queues::OutgoingActivityJobData;

const DELIVERER_TIMEOUT: u64 = 30;

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

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
}

fn build_client(instance: &Instance) -> reqwest::Result<Client> {
    let mut client_builder = Client::builder();
    if let Some(ref proxy_url) = instance.proxy_url {
        let proxy = Proxy::all(proxy_url)?;
        client_builder = client_builder.proxy(proxy);
    };
    let timeout = Duration::from_secs(DELIVERER_TIMEOUT);
    client_builder
        .timeout(timeout)
        .build()
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
            .chars().filter(|chr| *chr != '\n' && *chr != '\r').take(75)
            .collect();
        log::info!(
            "response from {}: [{}] {}",
            inbox_url,
            response_status.as_str(),
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

#[derive(Deserialize, Serialize)]
pub struct Recipient {
    id: String,
    inbox: String,
}

async fn deliver_activity_worker(
    maybe_db_pool: Option<DbPool>,
    instance: Instance,
    sender: User,
    activity: Value,
    recipients: Vec<Recipient>,
) -> Result<(), DelivererError> {
    let actor_key = deserialize_private_key(&sender.private_key)?;
    let actor_id = local_actor_id(
        &instance.url(),
        &sender.profile.username,
    );
    let actor_key_id = local_actor_key_id(&actor_id);
    let activity_signed = if is_object_signed(&activity) {
        log::warn!("activity is already signed");
        activity
    } else {
        sign_object(&activity, &actor_key, &actor_key_id)?
    };
    let activity_json = serde_json::to_string(&activity_signed)?;

    if recipients.is_empty() {
        return Ok(());
    };
    let mut queue: Vec<_> = recipients.into_iter()
        // is_delivered: false
        .map(|recipient| (recipient, false))
        .collect();
    log::info!(
        "sending activity to {} inboxes: {}",
        queue.len(),
        activity_json,
    );

    let mut retry_count = 0;
    let max_retries = 2;

    while queue.iter().any(|(_, is_delivered)| !is_delivered) &&
        retry_count <= max_retries
    {
        if retry_count > 0 {
            // Wait before next attempt
            sleep(backoff(retry_count)).await;
        };
        for (recipient, is_delivered) in queue.iter_mut() {
            if *is_delivered {
                continue;
            };
            if let Err(error) = send_activity(
                &instance,
                &actor_key,
                &actor_key_id,
                &activity_json,
                &recipient.inbox,
            ).await {
                log::warn!(
                    "failed to deliver activity to {} (attempt #{}): {}",
                    recipient.inbox,
                    retry_count + 1,
                    error,
                );
            } else {
                *is_delivered = true;
            };
        };
        retry_count += 1;
    };

    if let Some(ref db_pool) = maybe_db_pool {
        // Get connection from pool only after finishing delivery
        let db_client = &**get_database_client(db_pool).await?;
        for (recipient, is_delivered) in queue {
            set_reachability_status(
                db_client,
                &recipient.id,
                is_delivered,
            ).await?;
        };
    };
    Ok(())
}

pub struct OutgoingActivity {
    pub db_pool: Option<DbPool>, // needed to track unreachable actors (optional)
    pub instance: Instance,
    pub sender: User,
    pub activity: Value,
    pub recipients: Vec<Recipient>,
}

impl OutgoingActivity {
    pub fn new(
        instance: &Instance,
        sender: &User,
        activity: impl Serialize,
        recipients: Vec<Actor>,
    ) -> Self {
        // Sort and de-duplicate recipients
        let mut recipient_map = BTreeMap::new();
        for actor in recipients {
            if !recipient_map.contains_key(&actor.id) {
                let recipient = Recipient {
                    id: actor.id.clone(),
                    inbox: actor.inbox,
                };
                recipient_map.insert(actor.id, recipient);
            };
        };
        Self {
            db_pool: None,
            instance: instance.clone(),
            sender: sender.clone(),
            activity: serde_json::to_value(activity)
                .expect("activity should be serializable"),
            recipients: recipient_map.into_values().collect(),
        }
    }

    pub(super) async fn deliver(
        self,
    ) -> Result<(), DelivererError> {
        deliver_activity_worker(
            self.db_pool,
            self.instance,
            self.sender,
            self.activity,
            self.recipients,
        ).await
    }

    pub(super) fn spawn_deliver(self) -> () {
        tokio::spawn(async move {
            self.deliver().await.unwrap_or_else(|err| {
                log::error!("{}", err);
            });
        });
    }

    pub async fn enqueue(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        let job_data = OutgoingActivityJobData {
            activity: self.activity,
            sender_id: self.sender.id,
            recipients: self.recipients,
        };
        job_data.into_job(db_client).await
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
