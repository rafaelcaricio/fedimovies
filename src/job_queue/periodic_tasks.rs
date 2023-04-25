use anyhow::Error;

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DbPool},
    emojis::queries::{delete_emoji, find_unused_remote_emojis},
    posts::queries::{delete_post, find_extraneous_posts},
    profiles::queries::{delete_profile, find_empty_profiles, get_profile_by_id},
};
use mitra_models::database::DatabaseError;
use mitra_models::notifications::queries::{delete_notification, get_mention_notifications};
use mitra_models::posts::queries::create_post;
use mitra_models::posts::types::PostCreateData;
use mitra_models::users::queries::get_user_by_id;
use mitra_utils::datetime::days_before_now;
use crate::activitypub::builders::announce::prepare_announce;

use crate::activitypub::queues::{
    process_queued_incoming_activities, process_queued_outgoing_activities,
};
use crate::media::remove_media;

pub async fn incoming_activity_queue_executor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    process_queued_incoming_activities(config, db_client).await?;
    Ok(())
}

pub async fn outgoing_activity_queue_executor(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    process_queued_outgoing_activities(config, db_pool).await?;
    Ok(())
}

pub async fn delete_extraneous_posts(config: &Config, db_pool: &DbPool) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let updated_before = match config.retention.extraneous_posts {
        Some(days) => days_before_now(days),
        None => return Ok(()), // not configured
    };
    let posts = find_extraneous_posts(db_client, &updated_before).await?;
    for post_id in posts {
        let deletion_queue = delete_post(db_client, &post_id).await?;
        remove_media(config, deletion_queue).await;
        log::info!("deleted post {}", post_id);
    }
    Ok(())
}

pub async fn delete_empty_profiles(config: &Config, db_pool: &DbPool) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let updated_before = match config.retention.empty_profiles {
        Some(days) => days_before_now(days),
        None => return Ok(()), // not configured
    };
    let profiles = find_empty_profiles(db_client, &updated_before).await?;
    for profile_id in profiles {
        let profile = get_profile_by_id(db_client, &profile_id).await?;
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        remove_media(config, deletion_queue).await;
        log::info!("deleted profile {}", profile.acct);
    }
    Ok(())
}

pub async fn prune_remote_emojis(config: &Config, db_pool: &DbPool) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let emojis = find_unused_remote_emojis(db_client).await?;
    for emoji_id in emojis {
        let deletion_queue = delete_emoji(db_client, &emoji_id).await?;
        remove_media(config, deletion_queue).await;
        log::info!("deleted emoji {}", emoji_id);
    }
    Ok(())
}

// Finds mention notifications and repost them
pub async fn handle_movies_mentions(config: &Config, db_pool: &DbPool) -> Result<(), anyhow::Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    log::debug!("Reviewing mentions..");
    // for each mention notification do repost
    let mut transaction = db_client.transaction().await?;

    let mention_notifications = match get_mention_notifications(&transaction, 50).await {
        Ok(mention_notifications) => mention_notifications,
        Err(DatabaseError::DatabaseClientError(err)) => {
            return Err(anyhow::anyhow!("Error in client: {err}"))
        }
        Err(err) => return Err(err.into()),
    };

    for mention_notification in mention_notifications {
        log::info!("Reviewing mention notification {}", mention_notification.id);
        if let Some(post_with_mention) = mention_notification.post {
            // Does not repost private posts or reposts
            if !post_with_mention.is_public() || post_with_mention.repost_of_id.is_some() {
                continue;
            }
            let mut post = post_with_mention.clone();
            let post_id = post.id;
            let current_user = get_user_by_id(&transaction, &mention_notification.recipient.id).await?;

            // Repost
            let repost_data = PostCreateData::repost(post.id, None);
            let mut repost = match create_post(&mut transaction, &current_user.id, repost_data).await {
                Ok(repost) => repost,
                Err(DatabaseError::AlreadyExists(err)) => {
                    log::info!("Review as Mention of {} already reposted the post with id {}", current_user.profile.username, post_id);
                    delete_notification(&mut transaction, mention_notification.id).await?;
                    continue;
                }
                Err(err) => return Err(err.into()),
            };
            post.repost_count += 1;
            repost.repost_of = Some(Box::new(post));

            // Federate
            prepare_announce(&transaction, &config.instance(), &current_user, &repost)
                .await?
                .enqueue(&mut transaction)
                .await?;

            // Delete notification to avoid re-processing
            delete_notification(&mut transaction, mention_notification.id).await?;

            log::info!("Review as Mention of {} reposted with post id {}", current_user.profile.username, post_id);
        }
    }
    Ok(transaction.commit().await?)
}
