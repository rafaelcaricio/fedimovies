use anyhow::Error;

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DbPool},
    emojis::queries::{
        delete_emoji,
        find_unused_remote_emojis,
    },
    posts::queries::{delete_post, find_extraneous_posts},
    profiles::queries::{
        delete_profile,
        find_empty_profiles,
        get_profile_by_id,
    },
};
use mitra_utils::datetime::days_before_now;

use crate::activitypub::queues::{
    process_queued_incoming_activities,
    process_queued_outgoing_activities,
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

pub async fn delete_extraneous_posts(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
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
    };
    Ok(())
}

pub async fn delete_empty_profiles(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
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
    };
    Ok(())
}

pub async fn prune_remote_emojis(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let emojis = find_unused_remote_emojis(db_client).await?;
    for emoji_id in emojis {
        let deletion_queue = delete_emoji(db_client, &emoji_id).await?;
        remove_media(config, deletion_queue).await;
        log::info!("deleted emoji {}", emoji_id);
    };
    Ok(())
}
