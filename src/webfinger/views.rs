use actix_web::{get, web, HttpResponse};

use mitra_config::{Config, Instance};
use mitra_models::{
    database::{get_database_client, DatabaseClient, DbPool},
    users::queries::is_registered_user,
};

use crate::activitypub::{
    constants::AP_MEDIA_TYPE,
    identifiers::{local_actor_id, local_instance_actor_id, parse_local_actor_id},
};
use crate::errors::{HttpError, ValidationError};
use crate::media::MediaStorage;
use crate::tmdb::lookup_and_create_movie_user;

use super::types::{
    ActorAddress, JsonResourceDescriptor, Link, WebfingerQueryParams, JRD_CONTENT_TYPE,
};

// https://datatracker.ietf.org/doc/html/rfc7565#section-7
fn parse_acct_uri(uri: &str) -> Result<ActorAddress, ValidationError> {
    let actor_address = uri
        .strip_prefix("acct:")
        .ok_or(ValidationError("invalid query target"))?
        .parse()?;
    Ok(actor_address)
}

async fn get_jrd(
    db_client: &impl DatabaseClient,
    instance: Instance,
    resource: &str,
) -> Result<JsonResourceDescriptor, HttpError> {
    let actor_address = if resource.starts_with("acct:") {
        parse_acct_uri(resource)?
    } else {
        // Actor ID? (reverse webfinger)
        let username = if resource == local_instance_actor_id(&instance.url()) {
            instance.hostname()
        } else {
            parse_local_actor_id(&instance.url(), resource)?
        };
        ActorAddress {
            username,
            hostname: instance.hostname(),
        }
    };
    if actor_address.hostname != instance.hostname() {
        // Wrong instance
        return Err(HttpError::NotFoundError("user"));
    };
    let actor_id = if actor_address.username == instance.hostname() {
        local_instance_actor_id(&instance.url())
    } else {
        if !is_registered_user(db_client, &actor_address.username).await? {
            return Err(HttpError::NotFoundError("user"));
        };
        local_actor_id(&instance.url(), &actor_address.username)
    };
    // Required by GNU Social
    let link_profile = Link {
        rel: "http://webfinger.net/rel/profile-page".to_string(),
        media_type: Some("text/html".to_string()),
        href: Some(actor_id.clone()),
        properties: Default::default(),
    };
    let link_actor = Link {
        rel: "self".to_string(),
        media_type: Some(AP_MEDIA_TYPE.to_string()),
        href: Some(actor_id),
        properties: Default::default(),
    };
    let jrd = JsonResourceDescriptor {
        subject: format!("acct:{}", actor_address),
        links: vec![link_profile, link_actor],
    };
    Ok(jrd)
}

#[get("/.well-known/webfinger")]
pub async fn webfinger_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<WebfingerQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let jrd = match get_jrd(db_client, config.instance(), &query_params.resource).await {
        Ok(jrd) => jrd,
        Err(_) => {
            // Lookup the movie in TMDB and create a local user. By now we know that the local
            // user for this movie does not exist.
            let config: &Config = &config;
            if let Some(api_key) = &config.tmdb_api_key {
                let movie_account = parse_acct_uri(&query_params.resource)?;
                let instance = config.instance();
                let storage = MediaStorage::from(config);
                lookup_and_create_movie_user(
                    &instance,
                    db_client,
                    api_key,
                    &storage.media_dir,
                    &movie_account.username,
                    config.movie_user_password.clone(),
                )
                .await
                .map_err(|err| {
                    log::error!("Failed to create movie user: {}", err);
                    HttpError::InternalError
                })?;
            }
            get_jrd(db_client, config.instance(), &query_params.resource).await?
        }
    };
    let response = HttpResponse::Ok().content_type(JRD_CONTENT_TYPE).json(jrd);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitra_models::{
        database::test_utils::create_test_database,
        users::{queries::create_user, types::UserCreateData},
    };
    use serial_test::serial;

    #[test]
    fn test_parse_acct_uri() {
        let uri = "acct:user_1@example.com";
        let actor_address = parse_acct_uri(uri).unwrap();
        assert_eq!(actor_address.username, "user_1");
        assert_eq!(actor_address.hostname, "example.com");
    }

    #[tokio::test]
    #[serial]
    async fn test_get_jrd() {
        let db_client = &mut create_test_database().await;
        let instance = Instance::for_test("https://example.com");
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        create_user(db_client, user_data).await.unwrap();
        let resource = "acct:test@example.com";
        let jrd = get_jrd(db_client, instance, resource).await.unwrap();
        assert_eq!(jrd.subject, resource);
        assert_eq!(
            jrd.links[0].href.as_ref().unwrap(),
            "https://example.com/users/test",
        );
    }
}
