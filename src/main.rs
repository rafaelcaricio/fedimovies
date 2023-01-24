use actix_cors::Cors;
use actix_web::{
    dev::Service,
    http::Method,
    middleware::Logger as ActixLogger,
    web,
    App,
    HttpResponse,
    HttpServer,
};
use tokio::sync::Mutex;

use mitra::activitypub::views as activitypub;
use mitra::atom::views as atom;
use mitra::config::{parse_config, Environment, MITRA_VERSION};
use mitra::database::{get_database_client, create_pool};
use mitra::database::migrate::apply_migrations;
use mitra::ethereum::contracts::get_contracts;
use mitra::http::json_error_handler;
use mitra::job_queue::scheduler;
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
use mitra::mastodon_api::settings::views::settings_api_scope;
use mitra::mastodon_api::statuses::views::status_api_scope;
use mitra::mastodon_api::subscriptions::views::subscription_api_scope;
use mitra::mastodon_api::timelines::views::timeline_api_scope;
use mitra::mastodon_api::UPLOAD_MAX_SIZE;
use mitra::nodeinfo::views as nodeinfo;
use mitra::webfinger::views as webfinger;
use mitra::web_client::views as web_client;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let (config, config_warnings) = parse_config();
    configure_logger(config.log_level);
    log::info!("config loaded from {}", config.config_path);
    for warning in config_warnings {
        log::warn!("{}", warning);
    };

    let db_pool = create_pool(&config.database_url);
    let mut db_client = get_database_client(&db_pool).await.unwrap();
    apply_migrations(&mut db_client).await;
    std::mem::drop(db_client);
    if !config.media_dir().exists() {
        std::fs::create_dir(config.media_dir())
            .expect("failed to create media directory");
    };
    log::info!(
        "app initialized; version {}, environment = '{:?}'",
        MITRA_VERSION,
        config.environment,
    );

    let maybe_blockchain = if let Some(blockchain_config) = config.blockchain() {
        if let Some(ethereum_config) = blockchain_config.ethereum_config() {
            // Create blockchain interface
            get_contracts(ethereum_config, &config.storage_dir).await
                .map(Some).unwrap()
        } else {
            None
        }
    } else {
        None
    };
    let maybe_contract_set = maybe_blockchain.clone()
        .map(|blockchain| blockchain.contract_set);

    scheduler::run(config.clone(), maybe_blockchain, db_pool.clone());
    log::info!("scheduler started");

    let num_workers = std::cmp::max(num_cpus::get(), 4);
    let http_socket_addr = format!(
        "{}:{}",
        config.http_host,
        config.http_port,
    );
    // Mutex is used to make server process incoming activities sequentially
    let inbox_mutex = web::Data::new(Mutex::new(()));

    let http_server = HttpServer::new(move || {
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
                    .allowed_origin_fn(|origin, req_head| {
                        req_head.method == Method::GET ||
                        origin.as_bytes().starts_with(b"http://localhost:")
                    })
                    .allow_any_method()
                    .allow_any_header()
                    .expose_any_header()
            },
        };
        let mut app = App::new()
            .wrap(cors_config)
            .wrap(ActixLogger::new("%r : %s : %{r}a"))
            .wrap_fn(|req, srv| {
                // Always log server errors (500-599)
                let fut = srv.call(req);
                async {
                    let res = fut.await?;
                    if let Some(error) = res.response().error() {
                        if error.as_response_error().status_code().is_server_error() {
                            log::warn!(
                                "{} {} : {}",
                                res.request().method(),
                                res.request().path(),
                                error,
                            );
                        };
                    };
                    Ok(res)
                }
            })
            .wrap(create_auth_error_handler())
            .app_data(web::PayloadConfig::default().limit(UPLOAD_MAX_SIZE * 2))
            .app_data(web::JsonConfig::default()
                .limit(UPLOAD_MAX_SIZE * 2)
                .error_handler(json_error_handler)
            )
            .app_data(web::Data::new(config.clone()))
            .app_data(web::Data::new(db_pool.clone()))
            .app_data(web::Data::new(maybe_contract_set.clone()))
            .app_data(web::Data::clone(&inbox_mutex))
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
            .service(search_api_scope())
            .service(settings_api_scope())
            .service(status_api_scope())
            .service(subscription_api_scope())
            .service(timeline_api_scope())
            .service(webfinger::webfinger_view)
            .service(activitypub::actor_scope())
            .service(activitypub::instance_actor_scope())
            .service(activitypub::object_view)
            .service(activitypub::emoji_view)
            .service(activitypub::tag_view)
            .service(atom::get_atom_feed)
            .service(nodeinfo::get_nodeinfo)
            .service(nodeinfo::get_nodeinfo_2_0)
            .service(web_client::profile_page_redirect())
            .service(web_client::post_page_redirect())
            .service(
                // Fallback for well-known paths
                web::resource("/.well-known/{path}")
                    .to(HttpResponse::NotFound)
            );
        if let Some(blockchain_config) = config.blockchain() {
            if let Some(ethereum_config) = blockchain_config.ethereum_config() {
                // Serve artifacts if available
                app = app.service(actix_files::Files::new(
                    "/contracts",
                    &ethereum_config.contract_dir,
                ));
            };
        };
        if let Some(ref web_client_dir) = config.web_client_dir {
            app = app.service(web_client::static_service(web_client_dir));
        };
        app
    });

    log::info!("listening on {}", http_socket_addr);
    http_server
        .workers(num_workers)
        .bind(http_socket_addr)?
        .run()
        .await
}
