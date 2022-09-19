use tokio_postgres::GenericClient;

use crate::errors::DatabaseError;

pub async fn search_tags(
    db_client: &impl GenericClient,
    search_query: &str,
    limit: i64,
) -> Result<Vec<String>, DatabaseError> {
    let db_search_query = format!("%{}%", search_query);
    let rows = db_client.query(
        "
        SELECT tag_name
        FROM tag
        WHERE tag_name ILIKE $1
        LIMIT $2
        ",
        &[&db_search_query, &limit],
    ).await?;
    let tags: Vec<String> = rows.iter()
        .map(|row| row.try_get("tag_name"))
        .collect::<Result<_, _>>()?;
    Ok(tags)
}
