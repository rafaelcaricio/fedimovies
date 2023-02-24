use actix_web::{get, web, HttpResponse};

use mitra_config::{Config, Instance};

use crate::activitypub::{
    constants::AP_MEDIA_TYPE,
    identifiers::{
        local_actor_id,
        local_instance_actor_id,
        parse_local_actor_id,
    },
};
use crate::database::{get_database_client, DatabaseClient, DbPool};
use crate::errors::{HttpError, ValidationError};
use crate::models::users::queries::is_registered_user;
use super::types::{
    ActorAddress,
    Link,
    JsonResourceDescriptor,
    WebfingerQueryParams,
    JRD_CONTENT_TYPE,
};

// https://datatracker.ietf.org/doc/html/rfc7565#section-7
fn parse_acct_uri(uri: &str) -> Result<ActorAddress, ValidationError> {
    let actor_address = uri.strip_prefix("acct:")
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
        ActorAddress { username, hostname: instance.hostname() }
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
    };
    let link_actor = Link {
        rel: "self".to_string(),
        media_type: Some(AP_MEDIA_TYPE.to_string()),
        href: Some(actor_id),
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
    let db_client = &**get_database_client(&db_pool).await?;
    let jrd = get_jrd(
        db_client,
        config.instance(),
        &query_params.resource,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type(JRD_CONTENT_TYPE)
        .json(jrd);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

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
