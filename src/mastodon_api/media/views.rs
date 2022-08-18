use actix_web::{post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::mastodon_api::uploads::{UploadError, save_b64_file};
use crate::models::attachments::queries::create_attachment;
use super::types::{AttachmentCreateData, Attachment};

#[post("")]
async fn create_attachment_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    attachment_data: web::Json<AttachmentCreateData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let (file_name, media_type) = save_b64_file(
        &attachment_data.file,
        &config.media_dir(),
    ).map_err(|err| match err {
        UploadError::Base64DecodingError(err) => {
            HttpError::ValidationError(err.to_string())
        },
        _ => HttpError::InternalError,
    })?;
    let db_attachment = create_attachment(
        db_client,
        &current_user.id,
        file_name,
        media_type,
    ).await?;
    let attachment = Attachment::from_db(
        db_attachment,
        &config.instance_url(),
    );
    Ok(HttpResponse::Ok().json(attachment))
}

pub fn media_api_scope() -> Scope {
    web::scope("/api/v1/media")
        .service(create_attachment_view)
}
