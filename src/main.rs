use actix_cors::Cors;
use actix_session::CookieSession;
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
use mitra::mastodon_api::directory::views::profile_directory;
use mitra::mastodon_api::instance::views as instance_api;
use mitra::mastodon_api::media::views::media_api_scope;
use mitra::mastodon_api::oauth::auth::create_auth_error_handler;
use mitra::mastodon_api::oauth::views::oauth_api_scope;
use mitra::mastodon_api::search::views::search;
use mitra::mastodon_api::statuses::views::status_api_scope;
use mitra::mastodon_api::timelines::views as timeline_api;
use mitra::mastodon_api::users::views as user_api;
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
        let cookie_config = CookieSession::signed(config.cookie_secret_key.as_bytes())
            .name(config.cookie_name.clone())
            .max_age(86400 * 30)
            .secure(true);
        App::new()
            .wrap(ActixLogger::new("%r : %s : %{r}a"))
            .wrap(cors_config)
            .wrap(cookie_config)
            .wrap(create_auth_error_handler())
            .data(web::PayloadConfig::default().limit(MAX_UPLOAD_SIZE))
            .data(web::JsonConfig::default().limit(MAX_UPLOAD_SIZE))
            .data(config.clone())
            .data(db_pool.clone())
            .service(actix_files::Files::new(
                "/media",
                config.media_dir(),
            ))
            .service(actix_files::Files::new(
                "/contracts",
                config.contract_dir.clone(),
            ))
            .service(oauth_api_scope())
            .service(user_api::create_user_view)
            .service(user_api::login_view)
            .service(user_api::current_user_view)
            .service(user_api::logout_view)
            .service(profile_directory)
            .service(account_api_scope())
            .service(media_api_scope())
            .service(status_api_scope())
            .service(instance_api::instance)
            .service(search)
            .service(timeline_api::home_timeline)
            .service(webfinger::get_descriptor)
            .service(activitypub_scope())
            .service(get_object)
            .service(nodeinfo::get_nodeinfo)
            .service(nodeinfo::get_nodeinfo_2_0)
    })
    .workers(num_workers)
    .bind(http_socket_addr)?
    .run()
    .await
}
