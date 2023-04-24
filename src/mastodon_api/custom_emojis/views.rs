use actix_web::{dev::ConnectionInfo, get, web, HttpResponse, Scope};

use mitra_models::{
    database::{get_database_client, DbPool},
    emojis::queries::get_local_emojis,
};

use super::types::CustomEmoji;
use crate::http::get_request_base_url;
use crate::mastodon_api::errors::MastodonError;

/// https://docs.joinmastodon.org/methods/custom_emojis/
#[get("")]
async fn custom_emoji_list(
    connection_info: ConnectionInfo,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let base_url = get_request_base_url(connection_info);
    let emojis: Vec<CustomEmoji> = get_local_emojis(db_client)
        .await?
        .into_iter()
        .map(|db_emoji| CustomEmoji::from_db(&base_url, db_emoji))
        .collect();
    Ok(HttpResponse::Ok().json(emojis))
}

pub fn custom_emoji_api_scope() -> Scope {
    web::scope("/api/v1/custom_emojis").service(custom_emoji_list)
}
