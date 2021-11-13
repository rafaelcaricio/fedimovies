use std::convert::TryFrom;

use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::posts::helpers::get_actions_for_posts;
use crate::models::posts::queries::{RELATED_ATTACHMENTS, RELATED_MENTIONS};
use super::types::{EventType, Notification};

async fn create_notification(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: Option<&Uuid>,
    event_type: EventType,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO notification (
            sender_id,
            recipient_id,
            post_id,
            event_type
        )
        VALUES ($1, $2, $3, $4)
        ",
        &[&sender_id, &recipient_id, &post_id, &i16::from(event_type)],
    ).await?;
    Ok(())
}

pub async fn create_follow_notification(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::Follow,
    ).await
}

pub async fn create_reply_notification(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Reply,
    ).await
}

pub async fn create_reaction_notification(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Reaction,
    ).await
}

pub async fn get_notifications(
    db_client: &impl GenericClient,
    recipient_id: &Uuid,
) -> Result<Vec<Notification>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            notification, sender, post, post_author,
            {related_attachments},
            {related_mentions}
        FROM notification
        JOIN actor_profile AS sender
        ON notification.sender_id = sender.id
        LEFT JOIN post
        ON notification.post_id = post.id
        LEFT JOIN actor_profile AS post_author
        ON post.author_id = post_author.id
        WHERE recipient_id = $1
        ORDER BY notification.created_at DESC
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&recipient_id],
    ).await?;
    let mut notifications: Vec<Notification> = rows.iter()
        .map(Notification::try_from)
        .collect::<Result<_, _>>()?;
    let posts = notifications.iter_mut()
        .filter_map(|item| item.post.as_mut())
        .collect();
    get_actions_for_posts(db_client, recipient_id, posts).await?;
    Ok(notifications)
}
