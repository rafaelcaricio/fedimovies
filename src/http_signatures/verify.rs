use std::collections::HashMap;

use actix_web::{
    HttpRequest,
    http::{Method, Uri, header::HeaderMap},
};
use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use rsa::RsaPublicKey;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    fetcher::helpers::get_or_import_profile_by_actor_id,
    handlers::HandlerError,
};
use crate::config::Config;
use crate::errors::DatabaseError;
use crate::models::profiles::queries::get_profile_by_remote_actor_id;
use crate::models::profiles::types::DbActorProfile;
use crate::utils::crypto::{deserialize_public_key, verify_signature};

#[derive(thiserror::Error, Debug)]
pub enum VerificationError {
    #[error("{0}")]
    HeaderError(&'static str),

    #[error("{0}")]
    ParseError(&'static str),

    #[error("invalid key ID")]
    UrlError(#[from] url::ParseError),

    #[error("database error")]
    DatabaseError(#[from] DatabaseError),

    #[error("{0}")]
    ActorError(String),

    #[error("invalid public key")]
    InvalidPublicKey(#[from] rsa::pkcs8::Error),

    #[error("invalid encoding")]
    InvalidEncoding(#[from] base64::DecodeError),

    #[error("invalid signature")]
    InvalidSignature,

    #[error("actor and request signer do not match")]
    InvalidSigner,
}

struct HttpSignatureData {
    pub key_id: String,
    pub message: String, // reconstructed message
    pub signature: String, // base64-encoded signature
    pub created_at: Option<DateTime<Utc>>,
}

const SIGNATURE_PARAMETER_RE: &str = r#"^(?P<key>[a-zA-Z]+)="(?P<value>.+)"$"#;

fn parse_http_signature(
    request_method: &Method,
    request_uri: &Uri,
    request_headers: &HeaderMap,
) -> Result<HttpSignatureData, VerificationError> {
    let signature_header = request_headers.get("signature")
        .ok_or(VerificationError::HeaderError("missing signature header"))?
        .to_str()
        .map_err(|_| VerificationError::HeaderError("invalid signature header"))?;

    let signature_parameter_re = Regex::new(SIGNATURE_PARAMETER_RE).unwrap();
    let mut signature_parameters = HashMap::new();
    for item in signature_header.split(',') {
        let caps = signature_parameter_re.captures(item)
            .ok_or(VerificationError::HeaderError("invalid signature header"))?;
        let key = caps["key"].to_string();
        let value = caps["value"].to_string();
        signature_parameters.insert(key, value);
    };

    let key_id = signature_parameters.get("keyId")
        .ok_or(VerificationError::ParseError("keyId parameter is missing"))?
        .to_owned();
    let headers_parameter = signature_parameters.get("headers")
        .ok_or(VerificationError::ParseError("headers parameter is missing"))?
        .to_owned();
    let signature = signature_parameters.get("signature")
        .ok_or(VerificationError::ParseError("signature is missing"))?
        .to_owned();
    let maybe_created_at = if let Some(created_at) = signature_parameters.get("created") {
        created_at.parse().ok().map(|ts| Utc.timestamp(ts, 0))
    } else {
        request_headers.get("date")
            .and_then(|header| header.to_str().ok())
            .and_then(|date| DateTime::parse_from_rfc2822(date).ok())
            .map(|datetime| datetime.with_timezone(&Utc))
    };

    let mut message_parts = vec![];
    for header in headers_parameter.split(' ') {
        let message_part = if header == "(request-target)" {
            format!(
                "(request-target): {} {}",
                request_method.as_str().to_lowercase(),
                request_uri,
            )
        } else if header == "(created)" {
            let created = signature_parameters.get("created")
                .ok_or(VerificationError::ParseError("created parameter is missing"))?;
            format!("(created): {}", created)
        } else if header == "(expires)" {
            let expires = signature_parameters.get("expires")
                .ok_or(VerificationError::ParseError("expires parameter is missing"))?;
            format!("(expires): {}", expires)
        } else {
            let header_value = request_headers.get(header)
                .ok_or(VerificationError::HeaderError("missing header"))?
                .to_str()
                .map_err(|_| VerificationError::HeaderError("invalid header value"))?;
            format!("{}: {}", header, header_value)
        };
        message_parts.push(message_part);
    };
    let message = message_parts.join("\n");

    let signature_data = HttpSignatureData {
        key_id,
        message,
        signature,
        created_at: maybe_created_at,
    };
    Ok(signature_data)
}

fn verify_http_signature(
    signature_data: &HttpSignatureData,
    signer_key: &RsaPublicKey,
) -> Result<(), VerificationError> {
    if signature_data.created_at.is_none() {
        log::warn!("signature creation time is missing");
    };
    let is_valid_signature = verify_signature(
        signer_key,
        &signature_data.message,
        &signature_data.signature,
    )?;
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(())
}

fn key_id_to_actor_id(key_id: &str) -> Result<String, url::ParseError> {
    let key_url = url::Url::parse(key_id)?;
    // Strip #main-key (works with most AP servers)
    let actor_id = &key_url[..url::Position::BeforeQuery];
    // GoToSocial compat
    let actor_id = actor_id.trim_end_matches("/main-key");
    Ok(actor_id.to_string())
}

/// Verifies HTTP signature and returns signer
pub async fn verify_signed_request(
    config: &Config,
    db_client: &impl GenericClient,
    request: &HttpRequest,
    no_fetch: bool,
) -> Result<DbActorProfile, VerificationError> {
    let signature_data = parse_http_signature(
        request.method(),
        request.uri(),
        request.headers(),
    )?;

    let actor_id = key_id_to_actor_id(&signature_data.key_id)?;
    let actor_profile = if no_fetch {
        get_profile_by_remote_actor_id(db_client, &actor_id).await?
    } else {
        match get_or_import_profile_by_actor_id(
            db_client,
            &config.instance(),
            &config.media_dir(),
            &actor_id,
        ).await {
            Ok(profile) => profile,
            Err(HandlerError::DatabaseError(error)) => return Err(error.into()),
            Err(other_error) => {
                return Err(VerificationError::ActorError(other_error.to_string()));
            },
        }
    };
    let actor = actor_profile.actor_json.as_ref()
        .ok_or(VerificationError::ActorError("invalid profile".to_string()))?;
    let public_key = deserialize_public_key(&actor.public_key.public_key_pem)?;

    verify_http_signature(&signature_data, &public_key)?;

    Ok(actor_profile)
}

#[cfg(test)]
mod tests {
    use actix_web::http::{
        header,
        header::{HeaderMap, HeaderName, HeaderValue},
        Uri,
    };
    use super::*;

    #[test]
    fn test_parse_signature() {
        let request_method = Method::POST;
        let request_uri = "/user/123/inbox".parse::<Uri>().unwrap();
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::HOST,
            HeaderValue::from_static("example.com"),
        );
        let signature_header = concat!(
            r#"keyId="https://myserver.org/actor#main-key","#,
            r#"headers="(request-target) host","#,
            r#"signature="test""#,
        );
        request_headers.insert(
            HeaderName::from_static("signature"),
            HeaderValue::from_static(signature_header),
        );

        let signature_data = parse_http_signature(
            &request_method,
            &request_uri,
            &request_headers,
        ).unwrap();
        assert_eq!(signature_data.key_id, "https://myserver.org/actor#main-key");
        assert_eq!(
            signature_data.message,
            "(request-target): post /user/123/inbox\nhost: example.com",
        );
        assert_eq!(signature_data.signature, "test");
        assert_eq!(signature_data.created_at.is_some(), false);
    }

    #[test]
    fn test_key_id_to_actor_id() {
        let key_id = "https://myserver.org/actor#main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://myserver.org/actor");

        let key_id = "https://myserver.org/actor/main-key";
        let actor_id = key_id_to_actor_id(key_id).unwrap();
        assert_eq!(actor_id, "https://myserver.org/actor");
    }
}
