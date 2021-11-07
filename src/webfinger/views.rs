use actix_web::{get, web, HttpResponse};
use regex::Regex;

use crate::activitypub::views::get_actor_url;
use crate::activitypub::constants::ACTIVITY_CONTENT_TYPE;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::models::users::queries::is_registered_user;
use super::types::{
    JRD_CONTENT_TYPE,
    WebfingerQueryParams,
    Link,
    JsonResourceDescriptor,
};

async fn get_user_info(
    db_pool: &Pool,
    config: &Config,
    query_params: WebfingerQueryParams,
) -> Result<JsonResourceDescriptor, HttpError> {
    // Parse 'acct' URI
    // https://datatracker.ietf.org/doc/html/rfc7565#section-7
    let uri_regexp = Regex::new(r"acct:(?P<user>\w+)@(?P<instance>.+)").unwrap();
    let uri_caps = uri_regexp.captures(&query_params.resource)
        .ok_or(HttpError::ValidationError("invalid query target".into()))?;
    let username = uri_caps.name("user")
        .ok_or(HttpError::ValidationError("invalid query target".into()))?
        .as_str();
    let instance_uri = uri_caps.name("instance")
        .ok_or(HttpError::ValidationError("invalid query target".into()))?
        .as_str();

    if instance_uri != config.instance_uri {
        // Wrong instance URI
        return Err(HttpError::NotFoundError("user"));
    }
    let db_client = &**get_database_client(db_pool).await?;
    if !is_registered_user(db_client, &username).await? {
        return Err(HttpError::NotFoundError("user"));
    }
    let actor_url = get_actor_url(&config.instance_url(), &username);
    let link = Link {
        rel: "self".to_string(),
        link_type: Some(ACTIVITY_CONTENT_TYPE.to_string()),
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
    db_bool: web::Data<Pool>,
    query_params: web::Query<WebfingerQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let jrd = get_user_info(&db_bool, &config, query_params.into_inner()).await?;
    let response = HttpResponse::Ok()
        .content_type(JRD_CONTENT_TYPE)
        .json(jrd);
    Ok(response)
}
