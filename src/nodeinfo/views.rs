/// http://nodeinfo.diaspora.software/protocol.html

use actix_web::{get, web, HttpResponse};

use mitra_config::Config;

use crate::database::{get_database_client, DbPool};
use crate::errors::HttpError;
use crate::webfinger::types::{
    Link,
    JsonResourceDescriptor,
};
use super::helpers::get_usage;
use super::types::NodeInfo20;

#[get("/.well-known/nodeinfo")]
pub async fn get_nodeinfo(
    config: web::Data<Config>,
) -> Result<HttpResponse, HttpError> {
    let nodeinfo_2_0_url = format!("{}/nodeinfo/2.0", config.instance_url());
    let link = Link {
        rel: "http://nodeinfo.diaspora.software/ns/schema/2.0".to_string(),
        media_type: None,
        href: Some(nodeinfo_2_0_url),
        properties: Default::default(),
    };
    let jrd = JsonResourceDescriptor {
        subject: config.instance_url(),
        links: vec![link],
    };
    let response = HttpResponse::Ok().json(jrd);
    Ok(response)
}

#[get("/nodeinfo/2.0")]
pub async fn get_nodeinfo_2_0(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let usage = get_usage(db_client).await?;
    let nodeinfo = NodeInfo20::new(&config, usage);
    let response = HttpResponse::Ok().json(nodeinfo);
    Ok(response)
}
