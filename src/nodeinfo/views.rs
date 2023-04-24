/// http://nodeinfo.diaspora.software/protocol.html
use actix_web::{get, web, HttpResponse};

use mitra_config::Config;
use mitra_models::database::{get_database_client, DbPool};

use super::helpers::get_usage;
use super::types::{NodeInfo20, NodeInfo21};
use crate::errors::HttpError;
use crate::webfinger::types::{JsonResourceDescriptor, Link};

#[get("/.well-known/nodeinfo")]
pub async fn get_nodeinfo_jrd(config: web::Data<Config>) -> Result<HttpResponse, HttpError> {
    let nodeinfo_2_0_url = format!("{}/nodeinfo/2.0", config.instance_url());
    let nodeinfo_2_0_link = Link {
        rel: "http://nodeinfo.diaspora.software/ns/schema/2.0".to_string(),
        media_type: None,
        href: Some(nodeinfo_2_0_url),
        properties: Default::default(),
    };
    let nodeinfo_2_1_url = format!("{}/nodeinfo/2.1", config.instance_url());
    let nodeinfo_2_1_link = Link {
        rel: "http://nodeinfo.diaspora.software/ns/schema/2.1".to_string(),
        media_type: None,
        href: Some(nodeinfo_2_1_url),
        properties: Default::default(),
    };
    let jrd = JsonResourceDescriptor {
        subject: config.instance_url(),
        links: vec![nodeinfo_2_0_link, nodeinfo_2_1_link],
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

#[get("/nodeinfo/2.1")]
pub async fn get_nodeinfo_2_1(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let usage = get_usage(db_client).await?;
    let nodeinfo = NodeInfo21::new(&config, usage);
    let response = HttpResponse::Ok().json(nodeinfo);
    Ok(response)
}
