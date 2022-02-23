use std::collections::HashMap;

use actix_web::{
    HttpRequest,
    http::{HeaderMap, Method, Uri},
};
use regex::Regex;
use tokio_postgres::GenericClient;

use crate::activitypub::fetcher::helpers::{
    get_or_import_profile_by_actor_id,
    ImportError,
};
use crate::config::Config;
use crate::errors::DatabaseError;
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
}

struct SignatureData {
    pub actor_id: String,
    pub message: String, // reconstructed message
    pub signature: String, // base64-encoded signature
}

const SIGNATURE_PARAMETER_RE: &str = r#"^(?P<key>[a-zA-Z]+)="(?P<value>.+)"$"#;

fn parse_http_signature(
    request_method: &Method,
    request_uri: &Uri,
    request_headers: &HeaderMap,
) -> Result<SignatureData, VerificationError> {
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
    db_client: &impl GenericClient,
    request: &HttpRequest,
) -> Result<DbActorProfile, VerificationError> {
    let signature_data = parse_http_signature(
        request.method(),
        request.uri(),
        request.headers(),
    )?;

    let actor_profile = match get_or_import_profile_by_actor_id(
        db_client,
        &config.instance(),
        &config.media_dir(),
        &signature_data.actor_id,
    ).await {
        Ok(profile) => profile,
        Err(ImportError::DatabaseError(error)) => return Err(error.into()),
        Err(other_error) => {
            return Err(VerificationError::ActorError(other_error.to_string()));
        },
    };
    let actor = actor_profile.actor_json.as_ref()
        .ok_or(VerificationError::ActorError("invalid profile".to_string()))?;

    let public_key = deserialize_public_key(&actor.public_key.public_key_pem)?;
    let is_valid_signature = verify_signature(
        &public_key,
        &signature_data.message,
        &signature_data.signature,
    )?;
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(actor_profile)
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
