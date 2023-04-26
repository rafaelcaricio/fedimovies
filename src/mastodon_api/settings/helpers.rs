use uuid::Uuid;

use fedimovies_config::Config;
use fedimovies_models::{
    database::{get_database_client, DatabaseClient, DatabaseError, DbPool},
    profiles::types::DbActorProfile,
    relationships::queries::{follow, get_followers, get_following, unfollow},
    users::types::User,
};

use crate::activitypub::{
    builders::{
        follow::follow_or_create_request, move_person::prepare_move_person,
        undo_follow::prepare_undo_follow,
    },
    fetcher::helpers::get_or_import_profile_by_actor_address,
    HandlerError,
};
use crate::errors::ValidationError;
use crate::media::MediaStorage;
use crate::webfinger::types::ActorAddress;

fn export_profiles_to_csv(local_hostname: &str, profiles: Vec<DbActorProfile>) -> String {
    let mut csv = String::new();
    for profile in profiles {
        let actor_address = ActorAddress::from_profile(local_hostname, &profile);
        csv += &format!("{}\n", actor_address);
    }
    csv
}

pub async fn export_followers(
    db_client: &impl DatabaseClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, followers);
    Ok(csv)
}

pub async fn export_follows(
    db_client: &impl DatabaseClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let following = get_following(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, following);
    Ok(csv)
}

pub fn parse_address_list(csv: &str) -> Result<Vec<ActorAddress>, ValidationError> {
    let mut addresses: Vec<_> = csv
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .map(|line| ActorAddress::from_mention(&line))
        .collect::<Result<_, _>>()?;
    addresses.sort();
    addresses.dedup();
    if addresses.len() > 50 {
        return Err(ValidationError(
            "can't process more than 50 items at once".to_string(),
        ));
    };
    Ok(addresses)
}

pub async fn import_follows_task(
    config: &Config,
    current_user: User,
    db_pool: &DbPool,
    address_list: Vec<ActorAddress>,
) -> Result<(), anyhow::Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let storage = MediaStorage::from(config);
    for actor_address in address_list {
        let profile = match get_or_import_profile_by_actor_address(
            db_client,
            &config.instance(),
            &storage,
            &actor_address,
        )
        .await
        {
            Ok(profile) => profile,
            Err(
                error @ (HandlerError::FetchError(_)
                | HandlerError::DatabaseError(DatabaseError::NotFound(_))),
            ) => {
                log::warn!("failed to import profile {}: {}", actor_address, error,);
                continue;
            }
            Err(other_error) => return Err(other_error.into()),
        };
        follow_or_create_request(db_client, &config.instance(), &current_user, &profile).await?;
    }
    Ok(())
}

pub async fn move_followers_task(
    config: &Config,
    db_pool: &DbPool,
    current_user: User,
    from_actor_id: &str,
    maybe_from_profile: Option<DbActorProfile>,
    address_list: Vec<ActorAddress>,
) -> Result<(), anyhow::Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let instance = config.instance();
    let storage = MediaStorage::from(config);
    let mut remote_followers = vec![];
    for follower_address in address_list {
        let follower = match get_or_import_profile_by_actor_address(
            db_client,
            &instance,
            &storage,
            &follower_address,
        )
        .await
        {
            Ok(profile) => profile,
            Err(
                error @ (HandlerError::FetchError(_)
                | HandlerError::DatabaseError(DatabaseError::NotFound(_))),
            ) => {
                log::warn!("failed to import profile {}: {}", follower_address, error,);
                continue;
            }
            Err(other_error) => return Err(other_error.into()),
        };
        if let Some(remote_actor) = follower.actor_json {
            // Add remote actor to activity recipients list
            remote_followers.push(remote_actor);
        } else {
            // Immediately move local followers (only if alias can be verified)
            if let Some(ref from_profile) = maybe_from_profile {
                match unfollow(db_client, &follower.id, &from_profile.id).await {
                    Ok(maybe_follow_request_id) => {
                        // Send Undo(Follow) to a remote actor
                        let remote_actor = from_profile
                            .actor_json
                            .as_ref()
                            .expect("actor data must be present");
                        let follow_request_id =
                            maybe_follow_request_id.expect("follow request must exist");
                        prepare_undo_follow(
                            &instance,
                            &current_user,
                            remote_actor,
                            &follow_request_id,
                        )
                        .enqueue(db_client)
                        .await?;
                    }
                    // Not a follower, ignore
                    Err(DatabaseError::NotFound(_)) => continue,
                    Err(other_error) => return Err(other_error.into()),
                };
                match follow(db_client, &follower.id, &current_user.id).await {
                    Ok(_) => (),
                    // Ignore if already following
                    Err(DatabaseError::AlreadyExists(_)) => (),
                    Err(other_error) => return Err(other_error.into()),
                };
            };
        };
    }
    prepare_move_person(
        &instance,
        &current_user,
        from_actor_id,
        remote_followers,
        None,
    )
    .enqueue(db_client)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fedimovies_models::profiles::types::DbActor;

    #[test]
    fn test_export_profiles_to_csv() {
        let profile_1 = DbActorProfile {
            username: "user1".to_string(),
            ..Default::default()
        };
        let profile_2 = DbActorProfile {
            username: "user2".to_string(),
            hostname: Some("test.net".to_string()),
            actor_json: Some(DbActor::default()),
            ..Default::default()
        };
        let csv = export_profiles_to_csv("example.org", vec![profile_1, profile_2]);
        assert_eq!(csv, "user1@example.org\nuser2@test.net\n");
    }

    #[test]
    fn test_parse_address_list() {
        let csv = concat!(
            "\nuser1@example.net\n",
            "user2@example.com  \n",
            "@user1@example.net",
        );
        let addresses = parse_address_list(csv).unwrap();
        assert_eq!(addresses.len(), 2);
        let addresses: Vec<_> = addresses
            .into_iter()
            .map(|address| address.to_string())
            .collect();
        assert_eq!(addresses, vec!["user1@example.net", "user2@example.com",]);
    }
}
