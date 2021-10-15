use std::convert::TryFrom;

use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::posts::types::DbPost;
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

pub async fn create_reply_notification(
    db_client: &impl GenericClient,
    reply: &DbPost,
) -> Result<(), DatabaseError> {
    let event_type: i16 = EventType::Reply.into();
    db_client.execute(
        "
        INSERT INTO notification (
            sender_id,
            recipient_id,
            post_id,
            event_type
        )
        SELECT $1, post.author_id, $2, $3
        FROM post WHERE id = $4
        ",
        &[
            &reply.author_id,
            &reply.id,
            &event_type,
            &reply.in_reply_to_id,
        ],
    ).await?;
    Ok(())
}

pub async fn get_notifications(
    db_client: &impl GenericClient,
    recipient_id: &Uuid,
) -> Result<Vec<Notification>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT
            notification, sender, post, post_author,
            ARRAY(
                SELECT media_attachment
                FROM media_attachment WHERE post_id = post.id
            ) AS attachments
        FROM notification
        JOIN actor_profile AS sender
        ON notification.sender_id = sender.id
        LEFT JOIN post
        ON notification.post_id = post.id
        LEFT JOIN actor_profile AS post_author
        ON post.author_id = post_author.id
        WHERE recipient_id = $1
        ",
        &[&recipient_id],
    ).await?;
    let notifications: Vec<Notification> = rows.iter()
        .map(|row| Notification::try_from(row))
        .collect::<Result<_, _>>()?;
    Ok(notifications)
}
