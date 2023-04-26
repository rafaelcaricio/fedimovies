use deadpool_postgres::SslMode;
use openssl::ssl::{SslConnector, SslMethod};
use postgres_openssl::MakeTlsConnector;
use std::path::Path;
use tokio_postgres::config::Config as DatabaseConfig;
use tokio_postgres::error::{Error as PgError, SqlState};

pub mod int_enum;
pub mod json_macro;
pub mod migrate;
pub mod query_macro;

#[cfg(feature = "test-utils")]
pub mod test_utils;

pub type DbPool = deadpool_postgres::Pool;
pub use tokio_postgres::GenericClient as DatabaseClient;
use tokio_postgres::NoTls;

#[derive(thiserror::Error, Debug)]
#[error("database type error")]
pub struct DatabaseTypeError;

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error("database pool error")]
    DatabasePoolError(#[from] deadpool_postgres::PoolError),

    #[error("database query error")]
    DatabaseQueryError(#[from] postgres_query::Error),

    #[error("database client error")]
    DatabaseClientError(#[from] tokio_postgres::Error),

    #[error(transparent)]
    DatabaseTypeError(#[from] DatabaseTypeError),

    #[error("{0} not found")]
    NotFound(&'static str), // object type

    #[error("{0} already exists")]
    AlreadyExists(&'static str), // object type
}

pub async fn create_database_client(
    db_config: &DatabaseConfig,
    ca_file_path: Option<&Path>,
) -> tokio_postgres::Client {
    let client = if let Some(ca_file_path) = ca_file_path {
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        log::debug!("Using TLS CA file: {}", ca_file_path.display());
        builder.set_ca_file(ca_file_path).unwrap();
        let connector = MakeTlsConnector::new(builder.build());
        let (client, connection) = db_config.connect(connector).await.unwrap();
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::error!("connection with tls error: {}", err);
            };
        });
        client
    } else {
        let (client, connection) = db_config.connect(tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::error!("connection error: {}", err);
            };
        });
        client
    };

    client
}

pub fn create_pool(database_url: &str, ca_file_path: Option<&Path>, pool_size: usize) -> DbPool {
    let manager = if let Some(ca_file_path) = ca_file_path {
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        log::info!("Using TLS CA file: {}", ca_file_path.display());
        builder.set_ca_file(ca_file_path).unwrap();
        let connector = MakeTlsConnector::new(builder.build());
        deadpool_postgres::Manager::new(
            database_url.parse().expect("invalid database URL"),
            connector,
        )
    } else {
        deadpool_postgres::Manager::new(database_url.parse().expect("invalid database URL"), NoTls)
    };

    DbPool::builder(manager)
        .max_size(pool_size)
        .build()
        .unwrap()
}

pub async fn get_database_client(
    db_pool: &DbPool,
) -> Result<deadpool_postgres::Client, DatabaseError> {
    // Returns wrapped client
    // https://github.com/bikeshedder/deadpool/issues/56
    let client = db_pool.get().await?;
    Ok(client)
}

pub fn catch_unique_violation(object_type: &'static str) -> impl Fn(PgError) -> DatabaseError {
    move |err| {
        if let Some(code) = err.code() {
            if code == &SqlState::UNIQUE_VIOLATION {
                return DatabaseError::AlreadyExists(object_type);
            };
        };
        err.into()
    }
}
