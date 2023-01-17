use uuid::Uuid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};
use crate::models::notifications::queries::create_reaction_notification;
use crate::models::posts::queries::{
    update_reaction_count,
    get_post_author,
};
use crate::utils::id::new_uuid;
use super::types::DbReaction;

pub async fn create_reaction(
    db_client: &mut impl DatabaseClient,
    author_id: &Uuid,
    post_id: &Uuid,
    activity_id: Option<&String>,
) -> Result<DbReaction, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let reaction_id = new_uuid();
    // Reactions to reposts are not allowed
    let maybe_row = transaction.query_opt(
        "
        INSERT INTO post_reaction (id, author_id, post_id, activity_id)
        SELECT $1, $2, $3, $4
        WHERE NOT EXISTS (
            SELECT 1 FROM post
            WHERE post.id = $3 AND post.repost_of_id IS NOT NULL
        )
        RETURNING post_reaction
        ",
        &[&reaction_id, &author_id, &post_id, &activity_id],
    ).await.map_err(catch_unique_violation("reaction"))?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let reaction: DbReaction = row.try_get("post_reaction")?;
    update_reaction_count(&transaction, post_id, 1).await?;
    let post_author = get_post_author(&transaction, post_id).await?;
    if post_author.is_local() && post_author.id != *author_id {
        create_reaction_notification(
            &transaction,
            author_id,
            &post_author.id,
            post_id,
        ).await?;
    };
    transaction.commit().await?;
    Ok(reaction)
}

pub async fn get_reaction_by_remote_activity_id(
    db_client: &impl DatabaseClient,
    activity_id: &str,
) -> Result<DbReaction, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT post_reaction
        FROM post_reaction
        WHERE activity_id = $1
        ",
        &[&activity_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("reaction"))?;
    let reaction = row.try_get("post_reaction")?;
    Ok(reaction)
}

pub async fn delete_reaction(
    db_client: &mut impl DatabaseClient,
    author_id: &Uuid,
    post_id: &Uuid,
) -> Result<Uuid, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        DELETE FROM post_reaction
        WHERE author_id = $1 AND post_id = $2
        RETURNING post_reaction.id
        ",
        &[&author_id, &post_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("reaction"))?;
    let reaction_id = row.try_get("id")?;
    update_reaction_count(&transaction, post_id, -1).await?;
    transaction.commit().await?;
    Ok(reaction_id)
}

/// Finds favourites among given posts and returns their IDs
pub async fn find_favourited_by_user(
    db_client: &impl DatabaseClient,
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
