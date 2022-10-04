use actix_web::{get, web, HttpResponse, Scope};

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::ethereum::contracts::ContractSet;
use crate::models::{
    instances::queries::get_peer_count,
    posts::queries::get_local_post_count,
    users::queries::get_user_count,
};
use super::types::InstanceInfo;

#[get("")]
async fn instance_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    maybe_blockchain: web::Data<Option<ContractSet>>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user_count = get_user_count(db_client).await?;
    let post_count = get_local_post_count(db_client).await?;
    let peer_count = get_peer_count(db_client).await?;
    let instance = InstanceInfo::create(
        config.as_ref(),
        maybe_blockchain.as_ref().as_ref(),
        user_count,
        post_count,
        peer_count,
    );
    Ok(HttpResponse::Ok().json(instance))
}

pub fn instance_api_scope() -> Scope {
    web::scope("/api/v1/instance")
        .service(instance_view)
}
