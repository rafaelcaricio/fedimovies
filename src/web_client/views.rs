use std::path::Path;

use actix_files::{Files, NamedFile};
use actix_web::{
    guard,
    web,
    HttpResponse,
    Resource,
    dev::{fn_service, ServiceRequest, ServiceResponse},
    web::Data,
};
use uuid::Uuid;

use crate::activitypub::{
    identifiers::{local_actor_id, local_object_id},
    views::is_activitypub_request,
};
use crate::config::Config;
use crate::database::{get_database_client, DbPool};
use crate::errors::HttpError;
use crate::models::users::queries::get_user_by_id;

pub fn static_service(web_client_dir: &Path) -> Files {
    Files::new("/", web_client_dir)
        .index_file("index.html")
        .prefer_utf8(true)
        .default_handler(fn_service(|service_request: ServiceRequest| {
            // Workaround for https://github.com/actix/actix-web/issues/2617
            let (request, _) = service_request.into_parts();
            let index_path = request.app_data::<Data<Config>>()
                .expect("app data should contain config")
                .web_client_dir.as_ref()
                .expect("web_client_dir should be present in config")
                .join("index.html");
            async {
                let index_file = NamedFile::open_async(index_path).await?;
                let response = index_file.into_response(&request);
                Ok(ServiceResponse::new(request, response))
            }
        }))
}

async fn profile_page_redirect_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    profile_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_id(db_client, &profile_id).await?;
    let actor_id = local_actor_id(
        &config.instance_url(),
        &user.profile.username,
    );
    let response = HttpResponse::Found()
        .append_header(("Location", actor_id))
        .finish();
    Ok(response)
}

pub fn profile_page_redirect() -> Resource {
    web::resource("/profile/{profile_id}")
        .guard(guard::fn_guard(|ctx| {
            is_activitypub_request(ctx.head().headers())
        }))
        .route(web::get().to(profile_page_redirect_view))
}

async fn post_page_redirect_view(
    config: web::Data<Config>,
    internal_object_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let object_id = local_object_id(
        &config.instance_url(),
        &internal_object_id,
    );
    let response = HttpResponse::Found()
        .append_header(("Location", object_id))
        .finish();
    Ok(response)
}

pub fn post_page_redirect() -> Resource {
    web::resource("/post/{object_id}")
        .guard(guard::fn_guard(|ctx| {
            is_activitypub_request(ctx.head().headers())
        }))
        .route(web::get().to(post_page_redirect_view))
}
