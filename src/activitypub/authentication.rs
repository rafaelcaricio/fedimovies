use actix_web::HttpRequest;
use tokio_postgres::GenericClient;

use crate::config::Config;
use crate::errors::DatabaseError;
use crate::http_signatures::verify::{
    parse_http_signature,
    verify_http_signature,
    HttpSignatureVerificationError as HttpSignatureError,
};
use crate::models::profiles::queries::get_profile_by_remote_actor_id;
use crate::models::profiles::types::DbActorProfile;
use crate::utils::crypto::deserialize_public_key;
use super::fetcher::helpers::get_or_import_profile_by_actor_id;
use super::receiver::HandlerError;

#[derive(thiserror::Error, Debug)]
pub enum AuthenticationError {
    #[error(transparent)]
    HttpSignatureError(#[from] HttpSignatureError),

    #[error("invalid key ID")]
    InvalidKeyId(#[from] url::ParseError),

    #[error("database error")]
    DatabaseError(#[from] DatabaseError),

    #[error("{0}")]
    ActorError(String),

    #[error("invalid public key")]
    InvalidPublicKey(#[from] rsa::pkcs8::Error),

    #[error("actor and request signer do not match")]
    InvalidSigner,
}

fn key_id_to_actor_id(key_id: &str) -> Result<String, AuthenticationError> {
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
) -> Result<DbActorProfile, AuthenticationError> {
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
                return Err(AuthenticationError::ActorError(other_error.to_string()));
            },
        }
    };
    let actor = actor_profile.actor_json.as_ref()
        .ok_or(AuthenticationError::ActorError("invalid profile".to_string()))?;
    let public_key = deserialize_public_key(&actor.public_key.public_key_pem)?;

    verify_http_signature(&signature_data, &public_key)?;

    Ok(actor_profile)
}

#[cfg(test)]
mod tests {
    use super::*;

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
