use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::posts::queries::update_reaction_count;

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
    transaction.commit().await?;
    Ok(())
}
