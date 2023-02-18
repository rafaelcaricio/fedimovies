use actix_web::{get, web, HttpResponse, Scope};

use mitra_config::Config;

use crate::database::{get_database_client, DbPool};
use crate::errors::HttpError;
use crate::models::emojis::queries::get_local_emojis;
use super::types::CustomEmoji;

/// https://docs.joinmastodon.org/methods/custom_emojis/
#[get("")]
async fn custom_emoji_list(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let instance = config.instance();
    let emojis: Vec<CustomEmoji> = get_local_emojis(db_client).await?
        .into_iter()
        .map(|db_emoji| CustomEmoji::from_db(&instance.url(), db_emoji))
        .collect();
    Ok(HttpResponse::Ok().json(emojis))
}

pub fn custom_emoji_api_scope() -> Scope {
    web::scope("/api/v1/custom_emojis")
        .service(custom_emoji_list)
}
