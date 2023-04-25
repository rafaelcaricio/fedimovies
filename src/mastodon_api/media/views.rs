/// https://docs.joinmastodon.org/methods/media/#v1
use actix_web::{post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use fedimovies_config::Config;
use fedimovies_models::{
    attachments::queries::create_attachment,
    database::{get_database_client, DbPool},
};

use super::types::{Attachment, AttachmentCreateData};
use crate::mastodon_api::{
    errors::MastodonError, oauth::auth::get_current_user, uploads::save_b64_file,
};

#[post("")]
async fn create_attachment_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    attachment_data: web::Json<AttachmentCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let (file_name, file_size, media_type) = save_b64_file(
        &attachment_data.file,
        attachment_data.media_type.clone(),
        &config.media_dir(),
        config.limits.media.file_size_limit,
        None,
    )?;
    let db_attachment = create_attachment(
        db_client,
        &current_user.id,
        file_name,
        file_size,
        Some(media_type),
    )
    .await?;
    let attachment = Attachment::from_db(&config.instance_url(), db_attachment);
    Ok(HttpResponse::Ok().json(attachment))
}

pub fn media_api_scope() -> Scope {
    web::scope("/api/v1/media").service(create_attachment_view)
}
