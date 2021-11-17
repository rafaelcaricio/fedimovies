use actix_web::http::Method;
use chrono::Utc;
use rsa::RsaPrivateKey;

use crate::utils::crypto::{sign_message, get_message_digest};

pub struct SignatureHeaders {
    pub host: String,
    pub date: String,
    pub digest: Option<String>,
    pub signature: String,
}

#[derive(thiserror::Error, Debug)]
pub enum SignatureError {
    #[error("invalid request url")]
    UrlError,

    #[error("signature error")]
    SignatureError(#[from] rsa::errors::Error),
}

/// Creates HTTP signature according to the old HTTP Signatures Spec:
/// https://datatracker.ietf.org/doc/html/draft-cavage-http-signatures.
pub fn create_http_signature(
    request_method: Method,
    request_url: &str,
    request_body: &str,
    actor_key: &RsaPrivateKey,
    actor_key_id: &str,
) -> Result<SignatureHeaders, SignatureError> {
    let request_url_object = url::Url::parse(request_url)
        .map_err(|_| SignatureError::UrlError)?;
    let request_target = format!(
        "{} {}",
        request_method.as_str().to_lowercase(),
        request_url_object.path(),
    );
    let host = request_url_object.host_str()
        .ok_or(SignatureError::UrlError)?
        .to_string();
    let date = Utc::now().format("%a, %d %b %Y %T GMT").to_string();
    let digest = if request_body.is_empty() {
        None
    } else {
        Some(get_message_digest(request_body))
    };

    let mut headers = vec![
        ("(request-target)", &request_target),
        ("host", &host),
        ("date", &date),
    ];
    if let Some(ref digest) = digest {
        headers.push(("digest", digest));
    };

    let message = headers.iter()
        .map(|(name, value)| format!("{}: {}", name, value))
        .collect::<Vec<String>>().join("\n");
    let headers_parameter = headers.iter()
        .map(|(name, _)| name.to_string())
        .collect::<Vec<String>>().join(" ");
    let signature_parameter = sign_message(actor_key, &message)?;
    let signature_header = format!(
        r#"keyId="{}",headers="{}",signature="{}""#,
        actor_key_id,
        headers_parameter,
        signature_parameter,
    );
    let headers = SignatureHeaders {
        host,
        date,
        digest,
        signature: signature_header,
    };
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;
    use super::*;

    #[test]
    fn test_create_signature_get() {
        let request_url = "https://example.org/inbox";
        let actor_key = RsaPrivateKey::new(&mut OsRng, 512).unwrap();
        let actor_key_id = "https://myserver.org/actor#main-key";

        let headers = create_http_signature(
            Method::GET,
            request_url,
            "",
            &actor_key,
            actor_key_id,
        ).unwrap();

        assert_eq!(headers.host, "example.org");
        assert_eq!(headers.digest, None);
        let expected_signature_header = concat!(
            r#"keyId="https://myserver.org/actor#main-key","#,
            r#"headers="(request-target) host date","#,
            r#"signature=""#,
        );
        assert_eq!(
            headers.signature.starts_with(expected_signature_header),
            true,
        );
    }

    #[test]
    fn test_create_signature_post() {
        let request_url = "https://example.org/inbox";
        let request_body = "{}";
        let actor_key = RsaPrivateKey::new(&mut OsRng, 512).unwrap();
        let actor_key_id = "https://myserver.org/actor#main-key";

        let result = create_http_signature(
            Method::POST,
            request_url,
            request_body,
            &actor_key,
            actor_key_id,
        );
        assert_eq!(result.is_ok(), true);

        let headers = result.unwrap();
        assert_eq!(headers.host, "example.org");
        assert_eq!(
            headers.digest.unwrap(),
            "SHA-256=RBNvo1WzZ4oRRq0W9+hknpT7T8If536DEMBg9hyq/4o=",
        );
        let expected_signature_header = concat!(
            r#"keyId="https://myserver.org/actor#main-key","#,
            r#"headers="(request-target) host date digest","#,
            r#"signature=""#,
        );
        assert_eq!(
            headers.signature.starts_with(expected_signature_header),
            true,
        );
    }
}
