use actix_web::{get, web, HttpResponse};
use regex::Regex;
use tokio_postgres::GenericClient;

use crate::activitypub::constants::AP_MEDIA_TYPE;
use crate::activitypub::identifiers::{
    local_actor_id,
    local_instance_actor_id,
};
use crate::config::{Config, Instance};
use crate::database::{Pool, get_database_client};
use crate::errors::{HttpError, ValidationError};
use crate::models::users::queries::is_registered_user;
use super::types::{
    JRD_CONTENT_TYPE,
    WebfingerQueryParams,
    Link,
    JsonResourceDescriptor,
};

async fn get_user_info(
    db_client: &impl GenericClient,
    instance: Instance,
    query_params: WebfingerQueryParams,
) -> Result<JsonResourceDescriptor, HttpError> {
    // Parse 'acct' URI
    // https://datatracker.ietf.org/doc/html/rfc7565#section-7
    // See also: USERNAME_RE in models::profiles::validators
    let uri_regexp = Regex::new(r"acct:(?P<user>[\w\.-]+)@(?P<instance>.+)").unwrap();
    let uri_caps = uri_regexp.captures(&query_params.resource)
        .ok_or(ValidationError("invalid query target"))?;
    let username = uri_caps.name("user")
        .ok_or(ValidationError("invalid query target"))?
        .as_str();
    let instance_host = uri_caps.name("instance")
        .ok_or(ValidationError("invalid query target"))?
        .as_str();

    if instance_host != instance.host() {
        // Wrong instance
        return Err(HttpError::NotFoundError("user"));
    }
    let actor_url = if username == instance.host() {
        local_instance_actor_id(&instance.url())
    } else {
        if !is_registered_user(db_client, username).await? {
            return Err(HttpError::NotFoundError("user"));
        };
        local_actor_id(&instance.url(), username)
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
    db_pool: web::Data<Pool>,
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
