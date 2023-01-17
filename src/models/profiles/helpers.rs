use crate::database::{DatabaseClient, DatabaseError};
use super::queries::search_profiles_by_did_only;
use super::types::DbActorProfile;

pub async fn find_aliases(
    db_client: &impl DatabaseClient,
    profile: &DbActorProfile,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let mut results = vec![];
    for identity_proof in profile.identity_proofs.inner() {
        let aliases = search_profiles_by_did_only(
            db_client,
            &identity_proof.issuer,
        ).await?;
        for alias in aliases {
            if alias.id == profile.id {
                continue;
            };
            results.push(alias);
        };
    };
    Ok(results)
}
