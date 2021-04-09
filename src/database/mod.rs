pub mod migrate;

pub type Pool = deadpool_postgres::Pool;

pub fn create_pool(database_url: &str) -> Pool {
    let pool = deadpool_postgres::Pool::new(
        deadpool_postgres::Manager::new(
            database_url.parse().expect("invalid database URL"),
            tokio_postgres::NoTls,
        ),
        // https://wiki.postgresql.org/wiki/Number_Of_Database_Connections
        num_cpus::get() * 2,
    );
    pool
}

use crate::errors::DatabaseError;

pub async fn get_database_client(pool: &Pool)
    -> Result<deadpool_postgres::Client, DatabaseError>
{
    // Returns wrapped client
    // https://github.com/bikeshedder/deadpool/issues/56
    let client = pool.get().await?;
    Ok(client)
}
