use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::notifications::queries::create_reaction_notification;
use crate::models::posts::queries::{get_post_by_id, update_reaction_count};

pub async fn create_reaction(
    db_client: &mut impl GenericClient,
    author_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let reaction_id = Uuid::new_v4();
    transaction.execute(
        "
        INSERT INTO post_reaction (id, author_id, post_id)
        VALUES ($1, $2, $3)
        ",
        &[&reaction_id, &author_id, &post_id],
    ).await.map_err(catch_unique_violation("reaction"))?;
    update_reaction_count(&transaction, post_id, 1).await?;
    let post = get_post_by_id(&transaction, post_id).await?;
    if post.author.is_local() {
        create_reaction_notification(
            &transaction,
            author_id,
            &post.author.id,
            &post.id,
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

pub async fn get_favourited(
    db_client: &impl GenericClient,
    user_id: &Uuid,
    posts_ids: Vec<Uuid>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post_id
        FROM post_reaction
        WHERE author_id = $1 AND post_id = ANY($2)
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let favourited: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    Ok(favourited)
}
