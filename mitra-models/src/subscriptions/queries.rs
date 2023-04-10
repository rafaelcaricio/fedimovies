use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_utils::caip2::ChainId;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};
use crate::invoices::types::DbChainId;
use crate::profiles::types::PaymentType;
use crate::relationships::{
    queries::{subscribe, subscribe_opt},
    types::RelationshipType,
};

use super::types::{DbSubscription, Subscription};

pub async fn create_subscription(
    db_client: &mut impl DatabaseClient,
    sender_id: &Uuid,
    sender_address: Option<&str>,
    recipient_id: &Uuid,
    chain_id: &ChainId,
    expires_at: &DateTime<Utc>,
    updated_at: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    assert!(chain_id.is_ethereum() == sender_address.is_some());
    let mut transaction = db_client.transaction().await?;
    transaction.execute(
        "
        INSERT INTO subscription (
            sender_id,
            sender_address,
            recipient_id,
            chain_id,
            expires_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
        &[
            &sender_id,
            &sender_address,
            &recipient_id,
            &DbChainId::new(chain_id),
            &expires_at,
            &updated_at,
        ],
    ).await.map_err(catch_unique_violation("subscription"))?;
    subscribe(&mut transaction, sender_id, recipient_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn update_subscription(
    db_client: &mut impl DatabaseClient,
    subscription_id: i32,
    expires_at: &DateTime<Utc>,
    updated_at: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        UPDATE subscription
        SET
            expires_at = $2,
            updated_at = $3
        WHERE id = $1
        RETURNING sender_id, recipient_id
        ",
        &[
            &subscription_id,
            &expires_at,
            &updated_at,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("subscription"))?;
    let sender_id: Uuid = row.try_get("sender_id")?;
    let recipient_id: Uuid = row.try_get("recipient_id")?;
    if *expires_at > Utc::now() {
        subscribe_opt(&mut transaction, &sender_id, &recipient_id).await?;
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn get_subscription_by_participants(
    db_client: &impl DatabaseClient,
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
    db_client: &impl DatabaseClient,
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
    db_client: &impl DatabaseClient,
    recipient_id: &Uuid,
    max_subscription_id: Option<i32>,
    limit: u16,
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
        &[&recipient_id, &max_subscription_id, &i64::from(limit)],
    ).await?;
    let subscriptions = rows.iter()
        .map(Subscription::try_from)
        .collect::<Result<_, _>>()?;
    Ok(subscriptions)
}

pub async fn reset_subscriptions(
    db_client: &mut impl DatabaseClient,
    ethereum_contract_replaced: bool,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    if ethereum_contract_replaced {
        // Ethereum subscription configuration is stored in contract.
        // If contract is replaced, payment option needs to be deleted.
        transaction.execute(
            "
            UPDATE actor_profile
            SET payment_options = '[]'
            WHERE
                actor_json IS NULL
                AND
                EXISTS (
                    SELECT 1
                    FROM jsonb_array_elements(payment_options) AS option
                    WHERE CAST(option ->> 'payment_type' AS SMALLINT) = $1
                )
            ",
            &[&i16::from(&PaymentType::EthereumSubscription)],
        ).await?;
    };
    transaction.execute(
        "
        DELETE FROM relationship
        WHERE relationship_type = $1
        ",
        &[&RelationshipType::Subscription],
    ).await?;
    transaction.execute(
        "UPDATE actor_profile SET subscriber_count = 0", &[],
    ).await?;
    transaction.execute("DELETE FROM subscription", &[]).await?;
    transaction.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::profiles::{
        queries::create_profile,
        types::ProfileCreateData,
    };
    use crate::relationships::{
        queries::has_relationship,
        types::RelationshipType,
    };
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_subscription() {
        let db_client = &mut create_test_database().await;
        let sender_data = ProfileCreateData {
            username: "sender".to_string(),
            ..Default::default()
        };
        let sender = create_profile(db_client, sender_data).await.unwrap();
        let sender_address = "0xb9c5714089478a327f09197987f16f9e5d936e8a";
        let recipient_data = UserCreateData {
            username: "recipient".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let recipient = create_user(db_client, recipient_data).await.unwrap();
        let chain_id = ChainId::ethereum_mainnet();
        let expires_at = Utc::now();
        let updated_at = Utc::now();
        create_subscription(
            db_client,
            &sender.id,
            Some(sender_address),
            &recipient.id,
            &chain_id,
            &expires_at,
            &updated_at,
        ).await.unwrap();

        let is_subscribed = has_relationship(
            db_client,
            &sender.id,
            &recipient.id,
            RelationshipType::Subscription,
        ).await.unwrap();
        assert_eq!(is_subscribed, true);
    }
}
