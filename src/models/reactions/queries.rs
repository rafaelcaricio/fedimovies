use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::notifications::queries::create_reaction_notification;
use crate::models::posts::queries::{
    update_reaction_count,
    get_post_author,
};
use crate::utils::id::new_uuid;

pub async fn create_reaction(
    db_client: &mut impl GenericClient,
    author_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let reaction_id = new_uuid();
    transaction.execute(
        "
        INSERT INTO post_reaction (id, author_id, post_id)
        VALUES ($1, $2, $3)
        ",
        &[&reaction_id, &author_id, &post_id],
    ).await.map_err(catch_unique_violation("reaction"))?;
    update_reaction_count(&transaction, post_id, 1).await?;
    let post_author = get_post_author(&transaction, post_id).await?;
    if post_author.is_local() && post_author.id != *author_id {
        create_reaction_notification(
            &transaction,
            author_id,
            &post_author.id,
            post_id,
        ).await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn delete_reaction(
    db_client: &mut impl GenericClient,
    author_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let deleted_count = transaction.execute(
        "
        DELETE FROM post_reaction
        WHERE author_id = $1 AND post_id = $2
        ",
        &[&author_id, &post_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("reaction"));
    }
    update_reaction_count(&transaction, post_id, -1).await?;
    transaction.commit().await?;
    Ok(())
}

/// Finds favourites among given posts and returns their IDs
pub async fn find_favourited_by_user(
    db_client: &impl GenericClient,
    user_id: &Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post_id
        FROM post_reaction
        WHERE author_id = $1 AND post_id = ANY($2)
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let favourites: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    Ok(favourites)
}
