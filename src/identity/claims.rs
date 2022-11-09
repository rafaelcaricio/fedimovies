use serde::Serialize;
use super::did::Did;

// https://www.w3.org/TR/vc-data-model/#credential-subject
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Claim {
    id: String, // actor ID
    owner_of: String, // DID
}

/// Creates key ownership claim and prepares it for signing
pub fn create_identity_claim(
    actor_id: &str,
    did: &Did,
) -> Result<String, serde_json::Error> {
    let claim = Claim {
        id: actor_id.to_string(),
        owner_of: did.to_string(),
    };
    let message = serde_json::to_string(&claim)?;
    Ok(message)
}
