use uuid::Uuid;

use crate::config::Instance;
use crate::database::{DatabaseClient, DatabaseError};
use crate::models::{
    emojis::types::DbEmoji,
    posts::{
        emojis::find_emojis,
        hashtags::{find_hashtags, replace_hashtags},
        helpers::{add_related_posts, add_user_actions},
        links::{replace_object_links, find_linked_posts},
        mentions::{find_mentioned_profiles, replace_mentions},
        types::Post,
    },
    users::types::User,
};
use super::types::Status;

pub struct PostContent {
    pub content: String,
    pub mentions: Vec<Uuid>,
    pub tags: Vec<String>,
    pub links: Vec<Uuid>,
    pub linked: Vec<Post>,
    pub emojis: Vec<DbEmoji>,
}

pub async fn parse_microsyntaxes(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    mut content: String,
) -> Result<PostContent, DatabaseError> {
    // Mentions
    let mention_map = find_mentioned_profiles(
        db_client,
        &instance.hostname(),
        &content,
    ).await?;
    content = replace_mentions(
        &mention_map,
        &instance.hostname(),
        &instance.url(),
        &content,
    );
    let mentions = mention_map.values().map(|profile| profile.id).collect();
    // Hashtags
    let tags = find_hashtags(&content);
    content = replace_hashtags(
        &instance.url(),
        &content,
        &tags,
    );
    // Links
    let link_map = find_linked_posts(
        db_client,
        &instance.url(),
        &content,
    ).await?;
    content = replace_object_links(
        &link_map,
        &content,
    );
    let links = link_map.values().map(|post| post.id).collect();
    let linked = link_map.into_values().collect();
    // Emojis
    let emoji_map = find_emojis(
        db_client,
        &content,
    ).await?;
    let emojis = emoji_map.into_values().collect();
    Ok(PostContent { content, mentions, tags, links, linked, emojis })
}

/// Load related objects and build status for API response
pub async fn build_status(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    user: Option<&User>,
    mut post: Post,
) -> Result<Status, DatabaseError> {
    add_related_posts(db_client, vec![&mut post]).await?;
    if let Some(user) = user {
        add_user_actions(db_client, &user.id, vec![&mut post]).await?;
    };
    let status = Status::from_post(post, instance_url);
    Ok(status)
}

pub async fn build_status_list(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    user: Option<&User>,
    mut posts: Vec<Post>,
) -> Result<Vec<Status>, DatabaseError> {
    add_related_posts(db_client, posts.iter_mut().collect()).await?;
    if let Some(user) = user {
        add_user_actions(db_client, &user.id, posts.iter_mut().collect()).await?;
    };
    let statuses: Vec<Status> = posts
        .into_iter()
        .map(|post| Status::from_post(post, instance_url))
        .collect();
    Ok(statuses)
}
