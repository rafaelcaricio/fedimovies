use tokio_postgres::GenericClient;

use crate::errors::DatabaseError;

pub async fn create_instance(
    db_client: &impl GenericClient,
    hostname: &str,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO instance VALUES ($1)
        ON CONFLICT DO NOTHING
        ",
        &[&hostname],
    ).await?;
    Ok(())
}

pub async fn get_peer_count(
    db_client: &impl GenericClient,
) -> Result<i64, DatabaseError> {
    let row = db_client.query_one(
        "SELECT count(instance) FROM instance",
        &[],
    ).await?;
    let count = row.try_get("count")?;
    Ok(count)
}
