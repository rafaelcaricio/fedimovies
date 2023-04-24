use crate::database::{DatabaseClient, DatabaseError};

pub async fn search_tags(
    db_client: &impl DatabaseClient,
    search_query: &str,
    limit: u16,
) -> Result<Vec<String>, DatabaseError> {
    let db_search_query = format!("%{}%", search_query);
    let rows = db_client
        .query(
            "
        SELECT tag_name
        FROM tag
        WHERE tag_name ILIKE $1
        LIMIT $2
        ",
            &[&db_search_query, &i64::from(limit)],
        )
        .await?;
    let tags: Vec<String> = rows
        .iter()
        .map(|row| row.try_get("tag_name"))
        .collect::<Result<_, _>>()?;
    Ok(tags)
}
