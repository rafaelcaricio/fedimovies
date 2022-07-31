use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::relationships::queries::{subscribe, subscribe_opt};
use crate::models::relationships::types::RelationshipType;
use super::types::{DbSubscription, Subscription};

pub async fn create_subscription(
    db_client: &mut impl GenericClient,
    sender_id: &Uuid,
    sender_address: &str,
    recipient_id: &Uuid,
    expires_at: &DateTime<Utc>,
    updated_at: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    transaction.execute(
        "
        INSERT INTO subscription (
            sender_id,
            sender_address,
            recipient_id,
            expires_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5)
        ",
        &[
            &sender_id,
            &sender_address,
            &recipient_id,
            &expires_at,
            &updated_at,
        ],
    ).await.map_err(catch_unique_violation("subscription"))?;
    subscribe(&transaction, sender_id, recipient_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn update_subscription(
    db_client: &mut impl GenericClient,
    subscription_id: i32,
    expires_at: &DateTime<Utc>,
    updated_at: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        UPDATE subscription
        SET
            expires_at = $1,
            updated_at = $2
        WHERE id = $3
        RETURNING sender_id, recipient_id
        ",
        &[
            &expires_at,
            &updated_at,
            &subscription_id,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("subscription"))?;
    let sender_id: Uuid = row.try_get("sender_id")?;
    let recipient_id: Uuid = row.try_get("recipient_id")?;
    subscribe_opt(&transaction, &sender_id, &recipient_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn get_subscription_by_participants(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<DbSubscription, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT subscription
        FROM subscription
        WHERE sender_id = $1 AND recipient_id = $2
        ",
        &[sender_id, recipient_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("subscription"))?;
    let subscription: DbSubscription = row.try_get("subscription")?;
    Ok(subscription)
}

pub async fn get_expired_subscriptions(
    db_client: &impl GenericClient,
) -> Result<Vec<DbSubscription>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT subscription
        FROM subscription
        JOIN relationship
        ON (
            relationship.source_id = subscription.sender_id
            AND relationship.target_id = subscription.recipient_id
            AND relationship.relationship_type = $1
        )
        WHERE subscription.expires_at <= CURRENT_TIMESTAMP
        ",
        &[&RelationshipType::Subscription],
    ).await?;
   let subscriptions = rows.iter()
        .map(|row| row.try_get("subscription"))
        .collect::<Result<_, _>>()?;
    Ok(subscriptions)
}

pub async fn get_incoming_subscriptions(
    db_client: &impl GenericClient,
    recipient_id: &Uuid,
    max_subscription_id: Option<i32>,
    limit: i64,
) -> Result<Vec<Subscription>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT subscription, actor_profile AS sender
        FROM actor_profile
        JOIN subscription
        ON (actor_profile.id = subscription.sender_id)
        WHERE
            subscription.recipient_id = $1
            AND ($2::integer IS NULL OR subscription.id < $2)
        ORDER BY subscription.id DESC
        LIMIT $3
        ",
        &[&recipient_id, &max_subscription_id, &limit],
    ).await?;
    let subscriptions = rows.iter()
        .map(Subscription::try_from)
        .collect::<Result<_, _>>()?;
    Ok(subscriptions)
}
