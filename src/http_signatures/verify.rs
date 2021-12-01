use actix_web::{
    HttpRequest,
    http::{HeaderMap, Method, Uri},
};
use regex::Regex;

use crate::activitypub::fetcher::fetch_profile_by_actor_id;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::DatabaseError;
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    create_profile,
};
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

    #[error("invalid key")]
    InvalidKey(#[from] rsa::pkcs8::Error),

    #[error("invalid signature")]
    InvalidSignature,
}

pub struct SignatureData {
    pub actor_id: String,
    pub message: String, // reconstructed message
    pub signature: String, // base64-encoded signature
}

fn parse_http_signature(
    request_method: &Method,
    request_uri: &Uri,
    request_headers: &HeaderMap,
) -> Result<SignatureData, VerificationError> {
    let signature_header = request_headers.get("signature")
        .ok_or(VerificationError::HeaderError("missing signature header"))?
        .to_str()
        .map_err(|_| VerificationError::HeaderError("invalid signature header"))?;
    // TODO: support arbitrary parameter order
    let signature_header_regexp_raw = concat!(
        r#"keyId="(?P<key_id>.+)","#,
        r#"headers="(?P<headers>.+)","#,
        r#"signature="(?P<signature>.+)""#,
    );
    let signature_header_regexp = Regex::new(signature_header_regexp_raw).unwrap();
    let signature_header_caps = signature_header_regexp
        .captures(signature_header)
        .ok_or(VerificationError::HeaderError("invalid signature header"))?;
    let key_id = signature_header_caps.name("key_id")
        .ok_or(VerificationError::ParseError("keyId parameter is missing"))?
        .as_str()
        .to_owned();
    let headers_parameter = signature_header_caps.name("headers")
        .ok_or(VerificationError::ParseError("headers parameter is missing"))?
        .as_str()
        .to_owned();
    let signature = signature_header_caps.name("signature")
        .ok_or(VerificationError::ParseError("signature is missing"))?
        .as_str()
        .to_owned();

    let mut message = format!(
        "(request-target): {} {}",
        request_method.as_str().to_lowercase(),
        request_uri,
    );
    for header in headers_parameter.split(' ') {
        if header == "(request-target)" {
            continue;
        }
        let header_value = request_headers.get(header)
            .ok_or(VerificationError::HeaderError("missing header"))?
            .to_str()
            .map_err(|_| VerificationError::HeaderError("invalid header value"))?;
        let message_part = format!(
            "\n{}: {}",
            header,
            header_value,
        );
        message.push_str(&message_part);
    }

    let key_url = url::Url::parse(&key_id)?;
    let actor_id = &key_url[..url::Position::BeforeQuery];

    let signature_data = SignatureData {
        actor_id: actor_id.to_string(),
        message,
        signature,
    };
    Ok(signature_data)
}

/// Verifies HTTP signature and returns signer ID
pub async fn verify_http_signature(
    config: &Config,
    db_pool: &Pool,
    request: &HttpRequest,
) -> Result<String, VerificationError> {
    let signature_data = parse_http_signature(
        request.method(),
        request.uri(),
        request.headers(),
    )?;

    let db_client = &**get_database_client(db_pool).await?;
    let actor_profile = match get_profile_by_actor_id(db_client, &signature_data.actor_id).await {
        Ok(profile) => profile,
        Err(err) => match err {
            DatabaseError::NotFound(_) => {
                let profile_data = fetch_profile_by_actor_id(
                    &config.instance(),
                    &signature_data.actor_id,
                    &config.media_dir(),
                ).await.map_err(|err| {
                    VerificationError::ActorError(err.to_string())
                })?;
                let profile = create_profile(db_client, &profile_data).await?;
                profile
            },
            other_error => {
                return Err(other_error.into());
            },
        },
    };
    let actor = actor_profile.remote_actor().ok().flatten()
        .ok_or(VerificationError::ActorError("invalid profile".to_string()))?;

    let public_key = deserialize_public_key(&actor.public_key.public_key_pem)?;
    let is_valid_signature = verify_signature(
        &public_key,
        &signature_data.message,
        &signature_data.signature,
    ).map_err(|_| VerificationError::InvalidSignature)?;
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    }
    let signer_id = actor_profile.actor_id(&config.instance_url())
        .map_err(|_| VerificationError::ActorError("invalid profile".to_string()))?;
    Ok(signer_id)
}

#[cfg(test)]
mod tests {
    use actix_web::http::{header, HeaderMap, HeaderName, HeaderValue, Uri};
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
        assert_eq!(signature_data.actor_id, "https://myserver.org/actor");
        assert_eq!(
            signature_data.message,
            "(request-target): post /user/123/inbox\nhost: example.com",
        );
        assert_eq!(signature_data.signature, "test");
    }
}
