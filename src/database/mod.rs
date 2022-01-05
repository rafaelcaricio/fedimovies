use tokio_postgres::config::{Config as DbConfig};

pub mod int_enum;
pub mod migrate;

pub type Pool = deadpool_postgres::Pool;

pub async fn create_database_client(db_config: &DbConfig) -> tokio_postgres::Client {
    let (client, connection) = db_config.connect(tokio_postgres::NoTls)
        .await.unwrap();
    tokio::spawn(async move {
        if let Err(err) = connection.await {
            log::error!("connection error: {}", err);
        };
    });
    client
}

pub fn create_pool(database_url: &str) -> Pool {
    deadpool_postgres::Pool::new(
        deadpool_postgres::Manager::new(
            database_url.parse().expect("invalid database URL"),
            tokio_postgres::NoTls,
        ),
        // https://wiki.postgresql.org/wiki/Number_Of_Database_Connections
        num_cpus::get() * 2,
    )
}

use tokio_postgres::error::{Error as PgError, SqlState};
use crate::errors::DatabaseError;

pub async fn get_database_client(pool: &Pool)
    -> Result<deadpool_postgres::Client, DatabaseError>
{
    // Returns wrapped client
    // https://github.com/bikeshedder/deadpool/issues/56
    let client = pool.get().await?;
    Ok(client)
}

pub fn catch_unique_violation(
    object_type: &'static str,
) -> impl Fn(PgError) -> DatabaseError {
    move |err| {
        if let Some(code) = err.code() {
            if code == &SqlState::UNIQUE_VIOLATION {
                return DatabaseError::AlreadyExists(object_type);
            }
        }
        err.into()
    }
}
