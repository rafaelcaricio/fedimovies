use std::path::Path;

use actix_files::{Files, NamedFile};
use actix_web::{
    dev::{fn_service, ServiceRequest, ServiceResponse},
    web::Data,
};

use crate::config::Config;

pub fn web_client_service(web_client_dir: &Path) -> Files {
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
