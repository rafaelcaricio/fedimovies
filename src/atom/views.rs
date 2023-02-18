use actix_web::{get, web, HttpResponse};

use mitra_config::Config;

use crate::database::{get_database_client, DbPool};
use crate::errors::HttpError;
use crate::models::{
    posts::queries::get_posts_by_author,
    users::queries::get_user_by_name,
};
use super::feeds::make_feed;

const FEED_SIZE: u16 = 10;

#[get("/feeds/{username}")]
pub async fn get_atom_feed(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    username: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    // Posts are ordered by creation date
    let posts = get_posts_by_author(
        db_client,
        &user.id,
        None, // include only public posts
        false, // exclude replies
        false, // exclude reposts
        None,
        FEED_SIZE,
    ).await?;
    let feed = make_feed(
        &config.instance(),
        &user.profile,
        posts,
    );
    let response = HttpResponse::Ok()
        .content_type("application/atom+xml")
        .body(feed);
    Ok(response)
}
