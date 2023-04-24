use crate::database::{DatabaseClient, DatabaseError};

pub async fn create_instance(
    db_client: &impl DatabaseClient,
    hostname: &str,
) -> Result<(), DatabaseError> {
    db_client
        .execute(
            "
        INSERT INTO instance VALUES ($1)
        ON CONFLICT DO NOTHING
        ",
            &[&hostname],
        )
        .await?;
    Ok(())
}

pub async fn get_peers(db_client: &impl DatabaseClient) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client
        .query(
            "
        SELECT instance.hostname FROM instance
        ",
            &[],
        )
        .await?;
    let peers = rows
        .iter()
        .map(|row| row.try_get("hostname"))
        .collect::<Result<_, _>>()?;
    Ok(peers)
}

pub async fn get_peer_count(db_client: &impl DatabaseClient) -> Result<i64, DatabaseError> {
    let row = db_client
        .query_one("SELECT count(instance) FROM instance", &[])
        .await?;
    let count = row.try_get("count")?;
    Ok(count)
}
