use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::profiles::types::DbActorProfile;
use crate::models::relationships::queries::{get_followers, get_following};

fn export_profiles_to_csv(
    local_hostname: &str,
    profiles: Vec<DbActorProfile>,
) -> String {
    let mut csv = String::new();
    for profile in profiles {
        let actor_address = profile.actor_address(local_hostname);
        csv += &format!("{}\n", actor_address);
    };
    csv
}

pub async fn export_followers(
    db_client: &impl GenericClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, followers);
    Ok(csv)
}

pub async fn export_follows(
    db_client: &impl GenericClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let following = get_following(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, following);
    Ok(csv)
}

#[cfg(test)]
mod tests {
    use crate::activitypub::actors::types::Actor;
    use super::*;

    #[test]
    fn test_export_profiles_to_csv() {
        let profile_1 = DbActorProfile {
            username: "user1".to_string(),
            ..Default::default()
        };
        let profile_2 = DbActorProfile {
            username: "user2".to_string(),
            hostname: Some("test.net".to_string()),
            actor_json: Some(Actor::default()),
            ..Default::default()
        };
        let csv = export_profiles_to_csv(
            "example.org",
            vec![profile_1, profile_2],
        );
        assert_eq!(csv, "user1@example.org\nuser2@test.net\n");
    }
}
