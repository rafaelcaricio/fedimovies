use std::convert::TryFrom;

use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use super::types::{EventType, Notification};

pub async fn create_notification(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    event_type: EventType,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO notification (
            sender_id,
            recipient_id,
            event_type
        )
        VALUES ($1, $2, $3)
        ",
        &[&sender_id, &recipient_id, &i16::from(event_type)],
    ).await?;
    Ok(())
}

pub async fn get_notifications(
    db_client: &impl GenericClient,
    recipient_id: &Uuid,
) -> Result<Vec<Notification>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT notification, sender
        FROM notification
        JOIN actor_profile AS sender
        ON notification.sender_id = sender.id
        WHERE recipient_id = $1
        ",
        &[&recipient_id],
    ).await?;
    let notifications: Vec<Notification> = rows.iter()
        .map(|row| Notification::try_from(row))
        .collect::<Result<_, _>>()?;
    Ok(notifications)
}
