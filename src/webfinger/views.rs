use actix_web::{get, web, HttpResponse};
use tokio_postgres::GenericClient;

use crate::activitypub::actors::types::ActorAddress;
use crate::activitypub::constants::AP_MEDIA_TYPE;
use crate::activitypub::identifiers::{
    local_actor_id,
    local_instance_actor_id,
};
use crate::config::{Config, Instance};
use crate::database::{get_database_client, DbPool};
use crate::errors::{HttpError, ValidationError};
use crate::models::users::queries::is_registered_user;
use super::types::{
    JRD_CONTENT_TYPE,
    WebfingerQueryParams,
    Link,
    JsonResourceDescriptor,
};

// https://datatracker.ietf.org/doc/html/rfc7565#section-7
fn parse_acct_uri(uri: &str) -> Result<ActorAddress, ValidationError> {
    let actor_address = uri.strip_prefix("acct:")
        .ok_or(ValidationError("invalid query target"))?
        .parse()?;
    Ok(actor_address)
}

async fn get_user_info(
    db_client: &impl GenericClient,
    instance: Instance,
    query_params: WebfingerQueryParams,
) -> Result<JsonResourceDescriptor, HttpError> {
    let actor_address = parse_acct_uri(&query_params.resource)?;
    if actor_address.hostname != instance.hostname() {
        // Wrong instance
        return Err(HttpError::NotFoundError("user"));
    };
    let actor_url = if actor_address.username == instance.hostname() {
        local_instance_actor_id(&instance.url())
    } else {
        if !is_registered_user(db_client, &actor_address.username).await? {
            return Err(HttpError::NotFoundError("user"));
        };
        local_actor_id(&instance.url(), &actor_address.username)
    };
    let link = Link {
        rel: "self".to_string(),
        link_type: Some(AP_MEDIA_TYPE.to_string()),
        href: Some(actor_url),
    };
    let jrd = JsonResourceDescriptor {
        subject: query_params.resource,
        links: vec![link],
    };
    Ok(jrd)
}

#[get("/.well-known/webfinger")]
pub async fn get_descriptor(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<WebfingerQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let jrd = get_user_info(
        db_client,
        config.instance(),
        query_params.into_inner(),
    ).await?;
    let response = HttpResponse::Ok()
        .content_type(JRD_CONTENT_TYPE)
        .json(jrd);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_acct_uri() {
        let uri = "acct:user_1@example.com";
        let actor_address = parse_acct_uri(uri).unwrap();
        assert_eq!(actor_address.username, "user_1");
        assert_eq!(actor_address.hostname, "example.com");
    }
}
