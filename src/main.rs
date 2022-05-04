use actix_cors::Cors;
use actix_web::{
    web,
    App, HttpServer,
    middleware::Logger as ActixLogger,
};

use mitra::activitypub::views as activitypub;
use mitra::atom::views as atom;
use mitra::config::{Environment, parse_config};
use mitra::database::{get_database_client, create_pool};
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
    configure_logger(config.log_level);
    log::info!("config loaded from {}", config.config_path);
    let db_pool = create_pool(&config.database_url);
    let mut db_client = get_database_client(&db_pool).await.unwrap();
    apply_migrations(&mut **db_client).await;
    std::mem::drop(db_client);
    if !config.media_dir().exists() {
        std::fs::create_dir(config.media_dir())
            .expect("failed to create media directory");
    };
    log::info!(
        "app initialized; version {}, environment = '{:?}'",
        config.version,
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
                let mut cors_config = Cors::default();
                for origin in config.http_cors_allowlist.iter() {
                    cors_config = cors_config.allowed_origin(origin);
                };
                cors_config
                    .allowed_origin(&config.instance_url())
                    .allowed_origin_fn(|origin, _req_head| {
                        origin.as_bytes().starts_with(b"http://localhost:")
                    })
                    .allow_any_method()
                    .allow_any_header()
            },
        };
        let mut app = App::new()
            .wrap(ActixLogger::new("%r : %s : %{r}a"))
            .wrap(cors_config)
            .wrap(create_auth_error_handler())
            .app_data(web::PayloadConfig::default().limit(MAX_UPLOAD_SIZE))
            .app_data(web::JsonConfig::default().limit(MAX_UPLOAD_SIZE))
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(db_pool.clone()))
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
            .service(activitypub::actor_scope())
            .service(activitypub::instance_actor_scope())
            .service(activitypub::object_view)
            .service(atom::get_atom_feed)
            .service(nodeinfo::get_nodeinfo)
            .service(nodeinfo::get_nodeinfo_2_0);
        if let Some(blockchain_config) = &config.blockchain {
            // Serve artifacts if available
            app = app.service(actix_files::Files::new(
                "/contracts",
                &blockchain_config.contract_dir,
            ));
        }
        app
    })
    .workers(num_workers)
    .bind(http_socket_addr)?
    .run()
    .await
}
