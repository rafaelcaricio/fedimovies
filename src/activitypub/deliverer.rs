use std::collections::BTreeMap;

use actix_web::http::Method;
use reqwest::Client;
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use mitra_config::Instance;
use mitra_models::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
    profiles::types::DbActor,
    users::types::User,
};
use mitra_utils::{
    crypto_rsa::deserialize_private_key,
    urls::get_hostname,
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

use super::{
    constants::AP_MEDIA_TYPE,
    http_client::build_federation_client,
    identifiers::{local_actor_id, local_actor_key_id},
    queues::OutgoingActivityJobData,
};

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

    #[error("inavlid URL")]
    UrlError(#[from] url::ParseError),

    #[error(transparent)]
    RequestError(#[from] reqwest::Error),

    #[error("http error {0:?}")]
    HttpError(reqwest::StatusCode),
}

fn build_client(
    instance: &Instance,
    request_uri: &str,
) -> Result<Client, DelivererError> {
    let hostname = get_hostname(request_uri)?;
    let is_onion = hostname.ends_with(".onion");
    let client = build_federation_client(
        instance,
        is_onion,
        instance.deliverer_timeout,
    )?;
    Ok(client)
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

    let client = build_client(instance, inbox_url)?;
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

#[derive(Deserialize, Serialize)]
pub struct Recipient {
    pub id: String,
    inbox: String,
    #[serde(default)]
    pub is_delivered: bool, // default to false if serialized data contains no value
}

async fn deliver_activity_worker(
    instance: Instance,
    sender: User,
    activity: Value,
    recipients: &mut [Recipient],
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

    for recipient in recipients.iter_mut() {
        if recipient.is_delivered {
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
                "failed to deliver activity to {}: {}",
                recipient.inbox,
                error,
            );
        } else {
            recipient.is_delivered = true;
        };
    };
    Ok(())
}

pub struct OutgoingActivity {
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
        recipients: Vec<DbActor>,
    ) -> Self {
        // Sort and de-duplicate recipients
        let mut recipient_map = BTreeMap::new();
        for actor in recipients {
            if !recipient_map.contains_key(&actor.id) {
                let recipient = Recipient {
                    id: actor.id.clone(),
                    inbox: actor.inbox,
                    is_delivered: false,
                };
                recipient_map.insert(actor.id, recipient);
            };
        };
        Self {
            instance: instance.clone(),
            sender: sender.clone(),
            activity: serde_json::to_value(activity)
                .expect("activity should be serializable"),
            recipients: recipient_map.into_values().collect(),
        }
    }

    pub(super) async fn deliver(
        mut self,
    ) -> Result<Vec<Recipient>, DelivererError> {
        deliver_activity_worker(
            self.instance,
            self.sender,
            self.activity,
            &mut self.recipients,
        ).await?;
        Ok(self.recipients)
    }

    pub async fn enqueue(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        if self.recipients.is_empty() {
            return Ok(());
        };
        log::info!(
            "delivering activity to {} inboxes: {}",
            self.recipients.len(),
            self.activity,
        );
        let job_data = OutgoingActivityJobData {
            activity: self.activity,
            sender_id: self.sender.id,
            recipients: self.recipients,
            failure_count: 0,
        };
        job_data.into_job(db_client, 0).await
    }
}
