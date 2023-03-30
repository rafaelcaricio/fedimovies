use actix_web::HttpRequest;
use serde_json::Value;

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::queries::get_profile_by_remote_actor_id,
    profiles::types::DbActorProfile,
};
use mitra_utils::{
    crypto_rsa::deserialize_public_key,
    did::Did,
};

use crate::http_signatures::verify::{
    parse_http_signature,
    verify_http_signature,
    HttpSignatureVerificationError as HttpSignatureError,
};
use crate::json_signatures::{
    proofs::ProofType,
    verify::{
        get_json_signature,
        verify_ed25519_json_signature,
        verify_eip191_json_signature,
        verify_rsa_json_signature,
        JsonSignatureVerificationError as JsonSignatureError,
        JsonSigner,
    },
};
use crate::media::MediaStorage;

use super::fetcher::helpers::get_or_import_profile_by_actor_id;
use super::receiver::HandlerError;

#[derive(thiserror::Error, Debug)]
pub enum AuthenticationError {
    #[error(transparent)]
    HttpSignatureError(#[from] HttpSignatureError),

    #[error("no HTTP signature")]
    NoHttpSignature,

    #[error(transparent)]
    JsonSignatureError(#[from] JsonSignatureError),

    #[error("no JSON signature")]
    NoJsonSignature,

    #[error("invalid JSON signature type")]
    InvalidJsonSignatureType,

    #[error("invalid key ID")]
    InvalidKeyId(#[from] url::ParseError),

    #[error("database error")]
    DatabaseError(#[from] DatabaseError),

    #[error("{0}")]
    ImportError(String),

    #[error("{0}")]
    ActorError(&'static str),

    #[error("invalid public key")]
    InvalidPublicKey(#[from] rsa::pkcs8::Error),

    #[error("actor and request signer do not match")]
    UnexpectedSigner,
}

fn key_id_to_actor_id(key_id: &str) -> Result<String, AuthenticationError> {
    let key_url = url::Url::parse(key_id)?;
    // Strip #main-key (works with most AP servers)
    let actor_id = &key_url[..url::Position::BeforeQuery];
    // GoToSocial compat
    let actor_id = actor_id.trim_end_matches("/main-key");
    Ok(actor_id.to_string())
}

async fn get_signer(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    signer_id: &str,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signer = if no_fetch {
        // Avoid fetching (e.g. if signer was deleted)
        get_profile_by_remote_actor_id(db_client, signer_id).await?
    } else {
        match get_or_import_profile_by_actor_id(
            db_client,
            &config.instance(),
            &MediaStorage::from(config),
            signer_id,
        ).await {
            Ok(profile) => profile,
            Err(HandlerError::DatabaseError(error)) => return Err(error.into()),
            Err(other_error) => {
                return Err(AuthenticationError::ImportError(other_error.to_string()));
            },
        }
    };
    Ok(signer)
}

/// Verifies HTTP signature and returns signer
pub async fn verify_signed_request(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    request: &HttpRequest,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signature_data = match parse_http_signature(
        request.method(),
        request.uri(),
        request.headers(),
    ) {
        Ok(signature_data) => signature_data,
        Err(HttpSignatureError::NoSignature) => {
            return Err(AuthenticationError::NoHttpSignature);
        },
        Err(other_error) => return Err(other_error.into()),
    };

    let signer_id = key_id_to_actor_id(&signature_data.key_id)?;
    let signer = get_signer(config, db_client, &signer_id, no_fetch).await?;
    let signer_actor = signer.actor_json.as_ref()
        .expect("request should be signed by remote actor");
    let signer_key =
        deserialize_public_key(&signer_actor.public_key.public_key_pem)?;

    verify_http_signature(&signature_data, &signer_key)?;

    Ok(signer)
}

/// Verifies JSON signature and returns signer
pub async fn verify_signed_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: &Value,
    no_fetch: bool,
) -> Result<DbActorProfile, AuthenticationError> {
    let signature_data = match get_json_signature(activity) {
        Ok(signature_data) => signature_data,
        Err(JsonSignatureError::NoProof) => {
            return Err(AuthenticationError::NoJsonSignature);
        },
        Err(other_error) => return Err(other_error.into()),
    };
    // Signed activities must have `actor` property, to avoid situations
    // where signer is identified by DID but there is no matching
    // identity proof in the local database.
    let actor_id = activity["actor"].as_str()
        .ok_or(AuthenticationError::ActorError("unknown actor"))?;
    let actor_profile = get_signer(config, db_client, actor_id, no_fetch).await?;

    match signature_data.signer {
        JsonSigner::ActorKeyId(ref key_id) => {
            if signature_data.signature_type != ProofType::JcsRsaSignature {
                return Err(AuthenticationError::InvalidJsonSignatureType);
            };
            let signer_id = key_id_to_actor_id(key_id)?;
            if signer_id != actor_id {
                return Err(AuthenticationError::UnexpectedSigner);
            };
            let signer_actor = actor_profile.actor_json.as_ref()
                .expect("activity should be signed by remote actor");
            let signer_key =
                deserialize_public_key(&signer_actor.public_key.public_key_pem)?;
            verify_rsa_json_signature(&signature_data, &signer_key)?;
        },
        JsonSigner::Did(did) => {
            if !actor_profile.identity_proofs.any(&did) {
                return Err(AuthenticationError::UnexpectedSigner);
            };
            match signature_data.signature_type {
                ProofType::JcsEd25519Signature => {
                    let did_key = match did {
                        Did::Key(did_key) => did_key,
                        _ => return Err(AuthenticationError::InvalidJsonSignatureType),
                    };
                    verify_ed25519_json_signature(
                        &did_key,
                        &signature_data.message,
                        &signature_data.signature,
                    )?;
                },
                ProofType::JcsEip191Signature => {
                    let did_pkh = match did {
                        Did::Pkh(did_pkh) => did_pkh,
                        _ => return Err(AuthenticationError::InvalidJsonSignatureType),
                    };
                    verify_eip191_json_signature(
                        &did_pkh,
                        &signature_data.message,
                        &signature_data.signature,
                    )?;
                },
                _ => return Err(AuthenticationError::InvalidJsonSignatureType),
            };
        },
    };
    // Signer is actor
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
