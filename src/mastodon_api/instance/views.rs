use actix_web::{get, web, HttpResponse};

use crate::config::Config;
use crate::errors::HttpError;
use super::types::Instance;

#[get("/api/v1/instance")]
pub async fn instance(
    instance_config: web::Data<Config>,
) -> Result<HttpResponse, HttpError> {
    let instance = Instance::from(instance_config.as_ref());
    Ok(HttpResponse::Ok().json(instance))
}
