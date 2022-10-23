use std::collections::HashMap;

use actix_web::http::{Method, Uri, header::HeaderMap};
use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use rsa::RsaPublicKey;

use crate::errors::DatabaseError;
use crate::utils::crypto::verify_signature;

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

pub struct HttpSignatureData {
    pub key_id: String,
    pub message: String, // reconstructed message
    pub signature: String, // base64-encoded signature
    pub created_at: Option<DateTime<Utc>>,
}

const SIGNATURE_PARAMETER_RE: &str = r#"^(?P<key>[a-zA-Z]+)="(?P<value>.+)"$"#;

pub fn parse_http_signature(
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

pub fn verify_http_signature(
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
}
