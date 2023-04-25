use actix_web::{get, post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use fedimovies_models::{
    database::{get_database_client, DbPool},
    markers::queries::{create_or_update_marker, get_marker_opt},
    markers::types::Timeline,
};

use crate::mastodon_api::{errors::MastodonError, oauth::auth::get_current_user};

use super::types::{MarkerCreateData, MarkerQueryParams, Markers};

/// https://docs.joinmastodon.org/methods/timelines/markers/
#[get("")]
async fn get_marker_view(
    auth: BearerAuth,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<MarkerQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let timeline = query_params.to_timeline()?;
    let maybe_db_marker = get_marker_opt(db_client, &current_user.id, timeline).await?;
    let markers = Markers {
        notifications: maybe_db_marker.map(|db_marker| db_marker.into()),
    };
    Ok(HttpResponse::Ok().json(markers))
}

#[post("")]
async fn update_marker_view(
    auth: BearerAuth,
    db_pool: web::Data<DbPool>,
    marker_data: web::Json<MarkerCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let db_marker = create_or_update_marker(
        db_client,
        &current_user.id,
        Timeline::Notifications,
        marker_data.into_inner().notifications,
    )
    .await?;
    let markers = Markers {
        notifications: Some(db_marker.into()),
    };
    Ok(HttpResponse::Ok().json(markers))
}

pub fn marker_api_scope() -> Scope {
    web::scope("/api/v1/markers")
        .service(get_marker_view)
        .service(update_marker_view)
}
