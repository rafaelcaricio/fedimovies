use actix_web::{web, HttpResponse, Scope};

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DbPool},
    posts::queries::get_posts_by_author,
    users::queries::get_user_by_name,
};

use super::feeds::make_feed;
use crate::errors::HttpError;

const FEED_SIZE: u16 = 10;

async fn get_atom_feed(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    username: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    // Posts are ordered by creation date
    let posts = get_posts_by_author(
        db_client, &user.id, None,  // include only public posts
        false, // exclude replies
        false, // exclude reposts
        None, FEED_SIZE,
    )
    .await?;
    let feed = make_feed(&config.instance(), &user.profile, posts);
    let response = HttpResponse::Ok()
        .content_type("application/atom+xml")
        .body(feed);
    Ok(response)
}

pub fn atom_scope() -> Scope {
    web::scope("/feeds")
        .route("/users/{username}", web::get().to(get_atom_feed))
        .route("/{username}", web::get().to(get_atom_feed))
}
