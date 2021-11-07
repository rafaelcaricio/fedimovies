use actix_cors::Cors;
use actix_web::{
    web,
    App, HttpServer,
    middleware::Logger as ActixLogger,
};

use mitra::activitypub::views::{activitypub_scope, get_object};
use mitra::config::{Environment, parse_config};
use mitra::database::create_pool;
use mitra::database::migrate::apply_migrations;
use mitra::logger::configure_logger;
use mitra::mastodon_api::accounts::views::account_api_scope;
use mitra::mastodon_api::directory::views::directory_api_scope;
use mitra::mastodon_api::instance::views::instance_api_scope;
use mitra::mastodon_api::markers::views::marker_api_scope;
use mitra::mastodon_api::media::views::media_api_scope;
use mitra::mastodon_api::notifications::views::notification_api_scope;
use mitra::mastodon_api::oauth::auth::create_auth_error_handler;
use mitra::mastodon_api::oauth::views::oauth_api_scope;
use mitra::mastodon_api::search::views::search_api_scope;
use mitra::mastodon_api::statuses::views::status_api_scope;
use mitra::mastodon_api::timelines::views::timeline_api_scope;
use mitra::nodeinfo::views as nodeinfo;
use mitra::scheduler;
use mitra::webfinger::views as webfinger;

const MAX_UPLOAD_SIZE: usize = 1024 * 1024 * 10;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = parse_config();
    configure_logger();
    let db_pool = create_pool(&config.database_url);
    apply_migrations(&db_pool).await;
    if !config.media_dir().exists() {
        std::fs::create_dir(config.media_dir())
            .expect("failed to created media directory");
    }
    log::info!(
        "app initialized; environment = '{:?}'",
        config.environment,
    );

    scheduler::run(config.clone(), db_pool.clone());
    log::info!("scheduler started");

    let http_socket_addr = format!(
        "{}:{}",
        config.http_host,
        config.http_port,
    );
    let num_workers = std::cmp::max(num_cpus::get(), 4);
    HttpServer::new(move || {
        let cors_config = match config.environment {
            Environment::Development => {
                Cors::permissive()
            },
            Environment::Production => {
                let allowed_origin = config.instance_url();
                Cors::default().allowed_origin(&allowed_origin)
                    .allow_any_method()
                    .allow_any_header()
            },
        };
        let mut app = App::new()
            .wrap(ActixLogger::new("%r : %s : %{r}a"))
            .wrap(cors_config)
            .wrap(create_auth_error_handler())
            .data(web::PayloadConfig::default().limit(MAX_UPLOAD_SIZE))
            .data(web::JsonConfig::default().limit(MAX_UPLOAD_SIZE))
            .data(config.clone())
            .data(db_pool.clone())
            .service(actix_files::Files::new(
                "/media",
                config.media_dir(),
            ))
            .service(oauth_api_scope())
            .service(account_api_scope())
            .service(directory_api_scope())
            .service(instance_api_scope())
            .service(marker_api_scope())
            .service(media_api_scope())
            .service(notification_api_scope())
            .service(status_api_scope())
            .service(search_api_scope())
            .service(timeline_api_scope())
            .service(webfinger::get_descriptor)
            .service(activitypub_scope())
            .service(get_object)
            .service(nodeinfo::get_nodeinfo)
            .service(nodeinfo::get_nodeinfo_2_0);
        if let Some(contract_dir) = &config.ethereum_contract_dir {
            // Serve artifacts if available
            app = app.service(actix_files::Files::new(
                "/contracts",
                contract_dir,
            ));
        }
        app
    })
    .workers(num_workers)
    .bind(http_socket_addr)?
    .run()
    .await
}
