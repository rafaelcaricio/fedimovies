use actix_web::{get, web, HttpResponse, Scope};

use crate::config::Config;
use crate::errors::HttpError;
use super::types::InstanceInfo;

#[get("")]
async fn instance_view(
    config: web::Data<Config>,
) -> Result<HttpResponse, HttpError> {
    let instance = InstanceInfo::from(config.as_ref());
    Ok(HttpResponse::Ok().json(instance))
}

pub fn instance_api_scope() -> Scope {
    web::scope("/api/v1/instance")
        .service(instance_view)
}
