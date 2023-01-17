use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};
use crate::models::posts::helpers::{add_related_posts, add_user_actions};
use crate::models::posts::queries::{
    RELATED_ATTACHMENTS,
    RELATED_EMOJIS,
    RELATED_LINKS,
    RELATED_MENTIONS,
    RELATED_TAGS,
};
use super::types::{EventType, Notification};

async fn create_notification(
    db_client: &impl DatabaseClient,
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
        &[&sender_id, &recipient_id, &post_id, &event_type],
    ).await?;
    Ok(())
}

pub async fn create_follow_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::Follow,
    ).await
}

pub async fn create_reply_notification(
    db_client: &impl DatabaseClient,
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
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Reaction,
    ).await
}

pub async fn create_mention_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Mention,
    ).await
}

pub async fn create_repost_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    post_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, Some(post_id),
        EventType::Repost,
    ).await
}

pub async fn create_subscription_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::Subscription,
    ).await
}

pub async fn create_subscription_expiration_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::SubscriptionExpiration,
    ).await
}

pub async fn create_move_notification(
    db_client: &impl DatabaseClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
) -> Result<(), DatabaseError> {
    create_notification(
        db_client, sender_id, recipient_id, None,
        EventType::Move,
    ).await
}

pub async fn get_notifications(
    db_client: &impl DatabaseClient,
    recipient_id: &Uuid,
    max_id: Option<i32>,
    limit: u16,
) -> Result<Vec<Notification>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            notification, sender, post, post_author,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM notification
        JOIN actor_profile AS sender
        ON notification.sender_id = sender.id
        LEFT JOIN post
        ON notification.post_id = post.id
        LEFT JOIN actor_profile AS post_author
        ON post.author_id = post_author.id
        WHERE
            recipient_id = $1
            AND ($2::integer IS NULL OR notification.id < $2)
        ORDER BY notification.id DESC
        LIMIT $3
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        related_links=RELATED_LINKS,
        related_emojis=RELATED_EMOJIS,
    );
    let rows = db_client.query(
        &statement,
        &[&recipient_id, &max_id, &i64::from(limit)],
    ).await?;
    let mut notifications: Vec<Notification> = rows.iter()
        .map(Notification::try_from)
        .collect::<Result<_, _>>()?;
    add_related_posts(
        db_client,
        notifications.iter_mut()
            .filter_map(|item| item.post.as_mut())
            .collect(),
    ).await?;
    add_user_actions(
        db_client,
        recipient_id,
        notifications.iter_mut()
            .filter_map(|item| item.post.as_mut())
            .collect(),
    ).await?;
    Ok(notifications)
}
