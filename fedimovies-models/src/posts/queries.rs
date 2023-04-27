use chrono::{DateTime, Utc};
use uuid::Uuid;

use fedimovies_utils::id::generate_ulid;

use crate::attachments::{queries::set_attachment_ipfs_cid, types::DbMediaAttachment};
use crate::cleanup::{find_orphaned_files, find_orphaned_ipfs_objects, DeletionQueue};
use crate::database::{catch_unique_violation, query_macro::query, DatabaseClient, DatabaseError};
use crate::emojis::types::DbEmoji;
use crate::notifications::queries::{
    create_mention_notification, create_reply_notification, create_repost_notification,
};
use crate::profiles::{queries::update_post_count, types::DbActorProfile};
use crate::relationships::queries::is_muted;
use crate::relationships::types::RelationshipType;

use super::types::{DbPost, Post, PostCreateData, PostUpdateData, Visibility};

async fn create_post_attachments(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    author_id: &Uuid,
    attachments: Vec<Uuid>,
) -> Result<Vec<DbMediaAttachment>, DatabaseError> {
    let attachments_rows = db_client
        .query(
            "
        UPDATE media_attachment
        SET post_id = $1
        WHERE owner_id = $2 AND id = ANY($3)
        RETURNING media_attachment
        ",
            &[&post_id, &author_id, &attachments],
        )
        .await?;
    if attachments_rows.len() != attachments.len() {
        // Some attachments were not found
        return Err(DatabaseError::NotFound("attachment"));
    };
    let mut attachments: Vec<DbMediaAttachment> = attachments_rows
        .iter()
        .map(|row| row.try_get("media_attachment"))
        .collect::<Result<_, _>>()?;
    attachments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(attachments)
}

async fn create_post_mentions(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    mentions: Vec<Uuid>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let mentions_rows = db_client
        .query(
            "
        INSERT INTO mention (post_id, profile_id)
        SELECT $1, actor_profile.id FROM actor_profile WHERE id = ANY($2)
        RETURNING (
            SELECT actor_profile FROM actor_profile
            WHERE actor_profile.id = profile_id
        ) AS actor_profile
        ",
            &[&post_id, &mentions],
        )
        .await?;
    if mentions_rows.len() != mentions.len() {
        // Some profiles were not found
        return Err(DatabaseError::NotFound("profile"));
    };
    let profiles = mentions_rows
        .iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

async fn create_post_tags(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    tags: Vec<String>,
) -> Result<Vec<String>, DatabaseError> {
    db_client
        .execute(
            "
        INSERT INTO tag (tag_name)
        SELECT unnest($1::text[])
        ON CONFLICT (tag_name) DO NOTHING
        ",
            &[&tags],
        )
        .await?;
    let tags_rows = db_client
        .query(
            "
        INSERT INTO post_tag (post_id, tag_id)
        SELECT $1, tag.id FROM tag WHERE tag_name = ANY($2)
        RETURNING (SELECT tag_name FROM tag WHERE tag.id = tag_id)
        ",
            &[&post_id, &tags],
        )
        .await?;
    if tags_rows.len() != tags.len() {
        return Err(DatabaseError::NotFound("tag"));
    };
    let tags = tags_rows
        .iter()
        .map(|row| row.try_get("tag_name"))
        .collect::<Result<_, _>>()?;
    Ok(tags)
}

async fn create_post_links(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    links: Vec<Uuid>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let links_rows = db_client
        .query(
            "
        INSERT INTO post_link (source_id, target_id)
        SELECT $1, post.id FROM post WHERE id = ANY($2)
        RETURNING target_id
        ",
            &[&post_id, &links],
        )
        .await?;
    if links_rows.len() != links.len() {
        return Err(DatabaseError::NotFound("post"));
    };
    let links = links_rows
        .iter()
        .map(|row| row.try_get("target_id"))
        .collect::<Result<_, _>>()?;
    Ok(links)
}

async fn create_post_emojis(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    emojis: Vec<Uuid>,
) -> Result<Vec<DbEmoji>, DatabaseError> {
    let emojis_rows = db_client
        .query(
            "
        INSERT INTO post_emoji (post_id, emoji_id)
        SELECT $1, emoji.id FROM emoji WHERE id = ANY($2)
        RETURNING (
            SELECT emoji FROM emoji
            WHERE emoji.id = emoji_id
        )
        ",
            &[&post_id, &emojis],
        )
        .await?;
    if emojis_rows.len() != emojis.len() {
        return Err(DatabaseError::NotFound("emoji"));
    };
    let emojis = emojis_rows
        .iter()
        .map(|row| row.try_get("emoji"))
        .collect::<Result<_, _>>()?;
    Ok(emojis)
}

pub async fn create_post(
    db_client: &mut impl DatabaseClient,
    author_id: &Uuid,
    post_data: PostCreateData,
) -> Result<Post, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let post_id = generate_ulid();
    // Replying to reposts is not allowed
    // Reposting of other reposts or non-public posts is not allowed
    let insert_statement = format!(
        "
        INSERT INTO post (
            id, author_id, content,
            in_reply_to_id,
            repost_of_id,
            visibility,
            is_sensitive,
            object_id,
            created_at
        )
        SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9
        WHERE
        NOT EXISTS (
            SELECT 1 FROM post
            WHERE post.id = $4 AND post.repost_of_id IS NOT NULL
        )
        AND NOT EXISTS (
            SELECT 1 FROM post
            WHERE post.id = $5 AND (
                post.repost_of_id IS NOT NULL
                OR post.visibility != {visibility_public}
            )
        )
        RETURNING post
        ",
        visibility_public = i16::from(&Visibility::Public),
    );
    let maybe_post_row = transaction
        .query_opt(
            &insert_statement,
            &[
                &post_id,
                &author_id,
                &post_data.content,
                &post_data.in_reply_to_id,
                &post_data.repost_of_id,
                &post_data.visibility,
                &post_data.is_sensitive,
                &post_data.object_id,
                &post_data.created_at,
            ],
        )
        .await
        .map_err(catch_unique_violation("post"))?;
    // Return NotFound error if reply/repost is not allowed
    let post_row = maybe_post_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = post_row.try_get("post")?;

    // Create related objects
    let db_attachments = create_post_attachments(
        &transaction,
        &db_post.id,
        &db_post.author_id,
        post_data.attachments,
    )
    .await?;
    let db_mentions = create_post_mentions(&transaction, &db_post.id, post_data.mentions).await?;
    let db_tags = create_post_tags(&transaction, &db_post.id, post_data.tags).await?;
    let db_links = create_post_links(&transaction, &db_post.id, post_data.links).await?;
    let db_emojis = create_post_emojis(&transaction, &db_post.id, post_data.emojis).await?;

    // Update counters
    let author = update_post_count(&transaction, &db_post.author_id, 1).await?;
    let mut notified_users = vec![];
    if let Some(in_reply_to_id) = &db_post.in_reply_to_id {
        update_reply_count(&transaction, in_reply_to_id, 1).await?;
        let in_reply_to_author = get_post_author(&transaction, in_reply_to_id).await?;
        if in_reply_to_author.is_local() && in_reply_to_author.id != db_post.author_id {
            create_reply_notification(
                &transaction,
                &db_post.author_id,
                &in_reply_to_author.id,
                &db_post.id,
            )
            .await?;
            notified_users.push(in_reply_to_author.id);
        };
    };
    // Notify reposted
    if let Some(repost_of_id) = &db_post.repost_of_id {
        update_repost_count(&transaction, repost_of_id, 1).await?;
        let repost_of_author = get_post_author(&transaction, repost_of_id).await?;
        if repost_of_author.is_local()
            && !notified_users.contains(&repost_of_author.id)
            // Don't notify themselves that they reported their post
            && repost_of_author.id != db_post.author_id
        {
            // Don't create mention notification if the author is muted
            if is_muted(&transaction, &repost_of_author.id, &db_post.author_id).await? {
                log::warn!(
                    "User {} mentioned by muted author id {} on post id {}, ignoring mention..",
                    repost_of_author.username,
                    db_post.author_id,
                    db_post.id
                );
            } else {
                create_repost_notification(
                    &transaction,
                    &db_post.author_id,
                    &repost_of_author.id,
                    repost_of_id,
                )
                .await?;
            }
            notified_users.push(repost_of_author.id);
        };
    };
    // Notify mentioned users
    for profile in db_mentions.iter() {
        if profile.is_local() &&
            profile.id != db_post.author_id &&
            // Don't send mention notification to the author of parent post
            // or to the author of reposted post
            !notified_users.contains(&profile.id)
        {
            // Don't create mention notification if the author is muted
            if is_muted(&transaction, &profile.id, &db_post.author_id).await? {
                log::warn!(
                    "User {} mentioned by muted author {} in post id {}, ignoring mention..",
                    profile.username,
                    db_post.author_id,
                    db_post.id
                );
            } else {
                create_mention_notification(
                    &transaction,
                    &db_post.author_id,
                    &profile.id,
                    &db_post.id,
                )
                .await?;
            }
        };
    }
    // Construct post object
    let post = Post::new(
        db_post,
        author,
        db_attachments,
        db_mentions,
        db_tags,
        db_links,
        db_emojis,
    )?;
    transaction.commit().await?;
    Ok(post)
}

pub async fn update_post(
    db_client: &mut impl DatabaseClient,
    post_id: &Uuid,
    post_data: PostUpdateData,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Reposts and immutable posts can't be updated
    let maybe_row = transaction
        .query_opt(
            "
        UPDATE post
        SET
            content = $1,
            is_sensitive = $2,
            updated_at = $3
        WHERE id = $4
            AND repost_of_id IS NULL
            AND ipfs_cid IS NULL
        RETURNING post
        ",
            &[
                &post_data.content,
                &post_data.is_sensitive,
                &post_data.updated_at,
                &post_id,
            ],
        )
        .await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = row.try_get("post")?;

    // Delete and re-create related objects
    transaction
        .execute(
            "DELETE FROM media_attachment WHERE post_id = $1",
            &[&db_post.id],
        )
        .await?;
    transaction
        .execute("DELETE FROM mention WHERE post_id = $1", &[&db_post.id])
        .await?;
    transaction
        .execute("DELETE FROM post_tag WHERE post_id = $1", &[&db_post.id])
        .await?;
    transaction
        .execute("DELETE FROM post_link WHERE source_id = $1", &[&db_post.id])
        .await?;
    transaction
        .execute("DELETE FROM post_emoji WHERE post_id = $1", &[&db_post.id])
        .await?;
    create_post_attachments(
        &transaction,
        &db_post.id,
        &db_post.author_id,
        post_data.attachments,
    )
    .await?;
    create_post_mentions(&transaction, &db_post.id, post_data.mentions).await?;
    create_post_tags(&transaction, &db_post.id, post_data.tags).await?;
    create_post_links(&transaction, &db_post.id, post_data.links).await?;
    create_post_emojis(&transaction, &db_post.id, post_data.emojis).await?;

    transaction.commit().await?;
    Ok(())
}

pub const RELATED_ATTACHMENTS: &str = "ARRAY(
        SELECT media_attachment
        FROM media_attachment WHERE post_id = post.id
        ORDER BY media_attachment.created_at
    ) AS attachments";

pub const RELATED_MENTIONS: &str = "ARRAY(
        SELECT actor_profile
        FROM mention
        JOIN actor_profile ON mention.profile_id = actor_profile.id
        WHERE post_id = post.id
    ) AS mentions";

pub const RELATED_TAGS: &str = "ARRAY(
        SELECT tag.tag_name FROM tag
        JOIN post_tag ON post_tag.tag_id = tag.id
        WHERE post_tag.post_id = post.id
    ) AS tags";

pub const RELATED_LINKS: &str = "ARRAY(
        SELECT post_link.target_id FROM post_link
        WHERE post_link.source_id = post.id
    ) AS links";

pub const RELATED_EMOJIS: &str = "ARRAY(
        SELECT emoji
        FROM post_emoji
        JOIN emoji ON post_emoji.emoji_id = emoji.id
        WHERE post_emoji.post_id = post.id
    ) AS emojis";

fn build_visibility_filter() -> String {
    format!(
        "(
            post.author_id = $current_user_id
            OR post.visibility = {visibility_public}
            -- covers direct messages and subscribers-only posts
            OR EXISTS (
                SELECT 1 FROM mention
                WHERE post_id = post.id AND profile_id = $current_user_id
            )
            OR post.visibility = {visibility_followers} AND EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = $current_user_id
                    AND target_id = post.author_id
                    AND relationship_type = {relationship_follow}
            )
        )",
        visibility_public = i16::from(&Visibility::Public),
        visibility_followers = i16::from(&Visibility::Followers),
        relationship_follow = i16::from(&RelationshipType::Follow),
    )
}

pub async fn get_home_timeline(
    db_client: &impl DatabaseClient,
    current_user_id: &Uuid,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    // Select posts from follows, subscriptions,
    // posts where current user is mentioned
    // and user's own posts.
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            (
                post.author_id = $current_user_id
                OR (
                    -- is following or subscribed the post author
                    EXISTS (
                        SELECT 1 FROM relationship
                        WHERE
                            source_id = $current_user_id
                            AND target_id = post.author_id
                            AND relationship_type IN ({relationship_follow}, {relationship_subscription})
                    )
                    AND (
                        -- show posts
                        post.repost_of_id IS NULL
                        -- show reposts if they are not hidden
                        OR NOT EXISTS (
                            SELECT 1 FROM relationship
                            WHERE
                                source_id = $current_user_id
                                AND target_id = post.author_id
                                AND relationship_type = {relationship_hide_reposts}
                        )
                        -- show reposts of current user's posts
                        OR EXISTS (
                            SELECT 1 FROM post AS repost_of
                            WHERE repost_of.id = post.repost_of_id
                                AND repost_of.author_id = $current_user_id
                        )
                    )
                    AND (
                        -- show posts (top-level)
                        post.in_reply_to_id IS NULL
                        -- show replies if they are not hidden
                        OR NOT EXISTS (
                            SELECT 1 FROM relationship
                            WHERE
                                source_id = $current_user_id
                                AND target_id = post.author_id
                                AND relationship_type = {relationship_hide_replies}
                        )
                        -- show replies to current user's posts
                        OR EXISTS (
                            SELECT 1 FROM post AS in_reply_to
                            WHERE
                                in_reply_to.id = post.in_reply_to_id
                                AND in_reply_to.author_id = $current_user_id
                        )
                    )
                )
                OR EXISTS (
                    SELECT 1 FROM mention
                    WHERE post_id = post.id AND profile_id = $current_user_id
                )
            )
            -- author is not muted
            AND NOT EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = $current_user_id
                    AND target_id = post.author_id
                    AND relationship_type = {relationship_mute}
            )
            AND {visibility_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        related_links=RELATED_LINKS,
        related_emojis=RELATED_EMOJIS,
        relationship_follow=i16::from(&RelationshipType::Follow),
        relationship_subscription=i16::from(&RelationshipType::Subscription),
        relationship_hide_reposts=i16::from(&RelationshipType::HideReposts),
        relationship_hide_replies=i16::from(&RelationshipType::HideReplies),
        relationship_mute=i16::from(&RelationshipType::Mute),
        visibility_filter=build_visibility_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        current_user_id = current_user_id,
        max_post_id = max_post_id,
        limit = limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter().map(Post::try_from).collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_local_timeline(
    db_client: &impl DatabaseClient,
    current_user_id: &Uuid,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            actor_profile.actor_json IS NULL
            AND post.visibility = {visibility_public}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
        visibility_public = i16::from(&Visibility::Public),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        current_user_id = current_user_id,
        max_post_id = max_post_id,
        limit = limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter().map(Post::try_from).collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_related_posts(
    db_client: &impl DatabaseClient,
    posts_ids: Vec<Uuid>,
) -> Result<Vec<Post>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id IN (
            SELECT post.in_reply_to_id
            FROM post WHERE post.id = ANY($1)
            UNION ALL
            SELECT post.repost_of_id
            FROM post WHERE post.id = ANY($1)
            UNION ALL
            SELECT post_link.target_id
            FROM post_link WHERE post_link.source_id = ANY($1)
            UNION ALL
            SELECT post_link.target_id
            FROM post_link JOIN post ON (post.repost_of_id = post_link.source_id)
            WHERE post.id = ANY($1)
        )
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
    );
    let rows = db_client.query(&statement, &[&posts_ids]).await?;
    let posts: Vec<Post> = rows.iter().map(Post::try_from).collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts_by_author(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
    current_user_id: Option<&Uuid>,
    include_replies: bool,
    include_reposts: bool,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let mut condition = format!(
        "post.author_id = $profile_id
        AND {visibility_filter}
        AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)",
        visibility_filter = build_visibility_filter(),
    );
    if !include_replies {
        condition.push_str(" AND post.in_reply_to_id IS NULL");
    };
    if !include_reposts {
        condition.push_str(" AND post.repost_of_id IS NULL");
    };
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE {condition}
        ORDER BY post.created_at DESC
        LIMIT $limit
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
        condition = condition,
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        profile_id = profile_id,
        current_user_id = current_user_id,
        max_post_id = max_post_id,
        limit = limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter().map(Post::try_from).collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts_by_tag(
    db_client: &impl DatabaseClient,
    tag_name: &str,
    current_user_id: Option<&Uuid>,
    max_post_id: Option<Uuid>,
    limit: u16,
) -> Result<Vec<Post>, DatabaseError> {
    let tag_name = tag_name.to_lowercase();
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            EXISTS (
                SELECT 1 FROM post_tag JOIN tag ON post_tag.tag_id = tag.id
                WHERE post_tag.post_id = post.id AND tag.tag_name = $tag_name
            )
            AND {visibility_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
        visibility_filter = build_visibility_filter(),
    );
    let limit: i64 = limit.into();
    let query = query!(
        &statement,
        tag_name = tag_name,
        current_user_id = current_user_id,
        max_post_id = max_post_id,
        limit = limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter().map(Post::try_from).collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_post_by_id(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
    );
    let maybe_row = db_client.query_opt(&statement, &[&post_id]).await?;
    let post = match maybe_row {
        Some(row) => Post::try_from(&row)?,
        None => return Err(DatabaseError::NotFound("post")),
    };
    Ok(post)
}

/// Given a post ID, finds all items in thread.
/// Results are sorted by tree path.
pub async fn get_thread(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    current_user_id: Option<&Uuid>,
) -> Result<Vec<Post>, DatabaseError> {
    // TODO: limit recursion depth
    let statement = format!(
        "
        WITH RECURSIVE
        ancestors (id, in_reply_to_id) AS (
            SELECT post.id, post.in_reply_to_id FROM post
            WHERE post.id = $post_id
                AND post.repost_of_id IS NULL
                AND {visibility_filter}
            UNION ALL
            SELECT post.id, post.in_reply_to_id FROM post
            JOIN ancestors ON post.id = ancestors.in_reply_to_id
        ),
        thread (id, path) AS (
            SELECT ancestors.id, ARRAY[ancestors.id] FROM ancestors
            WHERE ancestors.in_reply_to_id IS NULL
            UNION
            SELECT post.id, array_append(thread.path, post.id) FROM post
            JOIN thread ON post.in_reply_to_id = thread.id
        )
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN thread ON post.id = thread.id
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE {visibility_filter}
        ORDER BY thread.path
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
        visibility_filter = build_visibility_filter(),
    );
    let query = query!(
        &statement,
        post_id = post_id,
        current_user_id = current_user_id,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter().map(Post::try_from).collect::<Result<_, _>>()?;
    if posts.is_empty() {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(posts)
}

pub async fn get_post_by_remote_object_id(
    db_client: &impl DatabaseClient,
    object_id: &str,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.object_id = $1
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
    );
    let maybe_row = db_client.query_opt(&statement, &[&object_id]).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let post = Post::try_from(&row)?;
    Ok(post)
}

pub async fn get_post_by_ipfs_cid(
    db_client: &impl DatabaseClient,
    ipfs_cid: &str,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags},
            {related_links},
            {related_emojis}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.ipfs_cid = $1
        ",
        related_attachments = RELATED_ATTACHMENTS,
        related_mentions = RELATED_MENTIONS,
        related_tags = RELATED_TAGS,
        related_links = RELATED_LINKS,
        related_emojis = RELATED_EMOJIS,
    );
    let result = db_client.query_opt(&statement, &[&ipfs_cid]).await?;
    let post = match result {
        Some(row) => Post::try_from(&row)?,
        None => return Err(DatabaseError::NotFound("post")),
    };
    Ok(post)
}

pub async fn update_reply_count(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client
        .execute(
            "
        UPDATE post
        SET reply_count = reply_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
            &[&change, &post_id],
        )
        .await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(())
}

pub async fn update_reaction_count(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client
        .execute(
            "
        UPDATE post
        SET reaction_count = reaction_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
            &[&change, &post_id],
        )
        .await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn update_repost_count(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client
        .execute(
            "
        UPDATE post
        SET repost_count = repost_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
            &[&change, &post_id],
        )
        .await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn set_post_ipfs_cid(
    db_client: &mut impl DatabaseClient,
    post_id: &Uuid,
    ipfs_cid: &str,
    attachments: Vec<(Uuid, String)>,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let updated_count = transaction
        .execute(
            "
        UPDATE post
        SET ipfs_cid = $1
        WHERE id = $2
            AND repost_of_id IS NULL
            AND ipfs_cid IS NULL
        ",
            &[&ipfs_cid, &post_id],
        )
        .await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    for (attachment_id, cid) in attachments {
        set_attachment_ipfs_cid(&transaction, &attachment_id, &cid).await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn set_post_token_id(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    token_id: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client
        .execute(
            "
        UPDATE post
        SET token_id = $1
        WHERE id = $2
            AND repost_of_id IS NULL
            AND token_id IS NULL
        ",
            &[&token_id, &post_id],
        )
        .await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn set_post_token_tx_id(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
    token_tx_id: &str,
) -> Result<(), DatabaseError> {
    let updated_count = db_client
        .execute(
            "
        UPDATE post
        SET token_tx_id = $1
        WHERE id = $2
            AND repost_of_id IS NULL
        ",
            &[&token_tx_id, &post_id],
        )
        .await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn get_post_author(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client
        .query_opt(
            "
        SELECT actor_profile
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
        ",
            &[&post_id],
        )
        .await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let author: DbActorProfile = row.try_get("actor_profile")?;
    Ok(author)
}

/// Finds reposts of given posts and returns their IDs
pub async fn find_reposts_by_user(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client
        .query(
            "
        SELECT post.id
        FROM post
        WHERE post.author_id = $1 AND post.repost_of_id = ANY($2)
        ",
            &[&user_id, &posts_ids],
        )
        .await?;
    let reposts: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(reposts)
}

/// Finds items reposted by user among given posts
pub async fn find_reposted_by_user(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client
        .query(
            "
        SELECT post.id
        FROM post
        WHERE post.id = ANY($2) AND EXISTS (
            SELECT 1 FROM post AS repost
            WHERE repost.author_id = $1 AND repost.repost_of_id = post.id
        )
        ",
            &[&user_id, &posts_ids],
        )
        .await?;
    let reposted: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(reposted)
}

pub async fn get_token_waitlist(
    db_client: &impl DatabaseClient,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client
        .query(
            "
        SELECT post.id
        FROM post
        WHERE token_tx_id IS NOT NULL AND token_id IS NULL
        ",
            &[],
        )
        .await?;
    let waitlist: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(waitlist)
}

/// Finds all contexts (identified by top-level post)
/// updated before the specified date
/// that do not contain local posts, reposts, mentions, links or reactions.
pub async fn find_extraneous_posts(
    db_client: &impl DatabaseClient,
    updated_before: &DateTime<Utc>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client
        .query(
            "
        WITH RECURSIVE context_post (id, post_id, created_at) AS (
            SELECT post.id, post.id, post.created_at
            FROM post
            WHERE
                post.in_reply_to_id IS NULL
                AND post.repost_of_id IS NULL
                AND post.created_at < $1
            UNION
            SELECT context_post.id, post.id, post.created_at
            FROM post
            JOIN context_post ON (
                post.in_reply_to_id = context_post.post_id
                OR post.repost_of_id = context_post.post_id
            )
        )
        SELECT context.id
        FROM (
            SELECT
                context_post.id,
                array_agg(context_post.post_id) AS posts,
                max(context_post.created_at) AS updated_at
            FROM context_post
            GROUP BY context_post.id
        ) AS context
        WHERE
            context.updated_at < $1
            AND NOT EXISTS (
                SELECT 1
                FROM post
                JOIN actor_profile ON post.author_id = actor_profile.id
                WHERE
                    post.id = ANY(context.posts)
                    AND actor_profile.actor_json IS NULL
            )
            AND NOT EXISTS (
                SELECT 1
                FROM mention
                JOIN actor_profile ON mention.profile_id = actor_profile.id
                WHERE
                    mention.post_id = ANY(context.posts)
                    AND actor_profile.actor_json IS NULL
            )
            AND NOT EXISTS (
                SELECT 1
                FROM post_reaction
                JOIN actor_profile ON post_reaction.author_id = actor_profile.id
                WHERE
                    post_reaction.post_id = ANY(context.posts)
                    AND actor_profile.actor_json IS NULL
            )
            AND NOT EXISTS (
                SELECT 1
                FROM post_link
                JOIN post ON post_link.source_id = post.id
                JOIN actor_profile ON post.author_id = actor_profile.id
                WHERE
                    post_link.target_id = ANY(context.posts)
                    AND actor_profile.actor_json IS NULL
            )
        ",
            &[&updated_before],
        )
        .await?;
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Deletes post from database and returns collection of orphaned objects.
pub async fn delete_post(
    db_client: &mut impl DatabaseClient,
    post_id: &Uuid,
) -> Result<DeletionQueue, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Select all posts that will be deleted.
    // This includes given post, its descendants and reposts.
    let posts_rows = transaction
        .query(
            "
        WITH RECURSIVE context (post_id) AS (
            SELECT post.id FROM post
            WHERE post.id = $1
            UNION
            SELECT post.id FROM post
            JOIN context ON (
                post.in_reply_to_id = context.post_id
                OR post.repost_of_id = context.post_id
            )
        )
        SELECT post_id FROM context
        ",
            &[&post_id],
        )
        .await?;
    let posts: Vec<Uuid> = posts_rows
        .iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    // Get list of attached files
    let files_rows = transaction
        .query(
            "
        SELECT file_name
        FROM media_attachment WHERE post_id = ANY($1)
        ",
            &[&posts],
        )
        .await?;
    let files: Vec<String> = files_rows
        .iter()
        .map(|row| row.try_get("file_name"))
        .collect::<Result<_, _>>()?;
    // Get list of linked IPFS objects
    let ipfs_objects_rows = transaction
        .query(
            "
        SELECT ipfs_cid
        FROM media_attachment
        WHERE post_id = ANY($1) AND ipfs_cid IS NOT NULL
        UNION ALL
        SELECT ipfs_cid
        FROM post
        WHERE id = ANY($1) AND ipfs_cid IS NOT NULL
        ",
            &[&posts],
        )
        .await?;
    let ipfs_objects: Vec<String> = ipfs_objects_rows
        .iter()
        .map(|row| row.try_get("ipfs_cid"))
        .collect::<Result<_, _>>()?;
    // Update post counters
    transaction
        .execute(
            "
        UPDATE actor_profile
        SET post_count = post_count - post.count
        FROM (
            SELECT post.author_id, count(*) FROM post
            WHERE post.id = ANY($1)
            GROUP BY post.author_id
        ) AS post
        WHERE actor_profile.id = post.author_id
        ",
            &[&posts],
        )
        .await?;
    // Delete post
    let maybe_post_row = transaction
        .query_opt(
            "
        DELETE FROM post WHERE id = $1
        RETURNING post
        ",
            &[&post_id],
        )
        .await?;
    let post_row = maybe_post_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = post_row.try_get("post")?;
    // Update counters
    if let Some(parent_id) = &db_post.in_reply_to_id {
        update_reply_count(&transaction, parent_id, -1).await?;
    }
    if let Some(repost_of_id) = &db_post.repost_of_id {
        update_repost_count(&transaction, repost_of_id, -1).await?;
    };
    let orphaned_files = find_orphaned_files(&transaction, files).await?;
    let orphaned_ipfs_objects = find_orphaned_ipfs_objects(&transaction, ipfs_objects).await?;
    transaction.commit().await?;
    Ok(DeletionQueue {
        files: orphaned_files,
        ipfs_objects: orphaned_ipfs_objects,
    })
}

pub async fn get_local_post_count(db_client: &impl DatabaseClient) -> Result<i64, DatabaseError> {
    let row = db_client
        .query_one(
            "
        SELECT count(post)
        FROM post
        JOIN user_account ON (post.author_id = user_account.id)
        WHERE post.in_reply_to_id IS NULL AND post.repost_of_id IS NULL
        ",
            &[],
        )
        .await?;
    let count = row.try_get("count")?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::test_utils::create_test_database;
    use crate::profiles::{queries::create_profile, types::ProfileCreateData};
    use crate::relationships::queries::{follow, hide_reposts, subscribe};
    use crate::users::{queries::create_user, types::UserCreateData};
    use chrono::Duration;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_create_post() {
        let db_client = &mut create_test_database().await;
        let author_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let author = create_profile(db_client, author_data).await.unwrap();
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, &author.id, post_data).await.unwrap();
        assert_eq!(post.content, "test post");
        assert_eq!(post.author.id, author.id);
        assert!(post.attachments.is_empty());
        assert!(post.mentions.is_empty());
        assert!(post.tags.is_empty());
        assert!(post.links.is_empty());
        assert_eq!(post.updated_at, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_post_with_link() {
        let db_client = &mut create_test_database().await;
        let author_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let author = create_profile(db_client, author_data).await.unwrap();
        let post_data_1 = PostCreateData::default();
        let post_1 = create_post(db_client, &author.id, post_data_1)
            .await
            .unwrap();
        let post_data_2 = PostCreateData {
            links: vec![post_1.id],
            ..Default::default()
        };
        let post_2 = create_post(db_client, &author.id, post_data_2)
            .await
            .unwrap();
        assert_eq!(post_2.links, vec![post_1.id]);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_post() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, &user.id, post_data).await.unwrap();
        let post_data = PostUpdateData {
            content: "test update".to_string(),
            updated_at: Utc::now(),
            ..Default::default()
        };
        update_post(db_client, &post.id, post_data).await.unwrap();
        let post = get_post_by_id(db_client, &post.id).await.unwrap();
        assert_eq!(post.content, "test update");
        assert!(post.updated_at.is_some());
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_post() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, &user.id, post_data).await.unwrap();
        let deletion_queue = delete_post(db_client, &post.id).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 0);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_home_timeline() {
        let db_client = &mut create_test_database().await;
        let current_user_data = UserCreateData {
            username: "test".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let current_user = create_user(db_client, current_user_data).await.unwrap();
        // Current user's post
        let post_data_1 = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post_1 = create_post(db_client, &current_user.id, post_data_1)
            .await
            .unwrap();
        // Current user's direct message
        let post_data_2 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let post_2 = create_post(db_client, &current_user.id, post_data_2)
            .await
            .unwrap();
        // Another user's public post
        let user_data_1 = UserCreateData {
            username: "another-user".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user_1 = create_user(db_client, user_data_1).await.unwrap();
        let post_data_3 = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post_3 = create_post(db_client, &user_1.id, post_data_3)
            .await
            .unwrap();
        // Direct message from another user to current user
        let post_data_4 = PostCreateData {
            content: "test post".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![current_user.id],
            ..Default::default()
        };
        let post_4 = create_post(db_client, &user_1.id, post_data_4)
            .await
            .unwrap();
        // Followers-only post from another user
        let post_data_5 = PostCreateData {
            content: "followers only".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_5 = create_post(db_client, &user_1.id, post_data_5)
            .await
            .unwrap();
        // Followed user's public post
        let user_data_2 = UserCreateData {
            username: "followed".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user_2 = create_user(db_client, user_data_2).await.unwrap();
        follow(db_client, &current_user.id, &user_2.id)
            .await
            .unwrap();
        let post_data_6 = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post_6 = create_post(db_client, &user_2.id, post_data_6)
            .await
            .unwrap();
        // Followed user's repost
        let post_data_7 = PostCreateData {
            repost_of_id: Some(post_3.id),
            ..Default::default()
        };
        let post_7 = create_post(db_client, &user_2.id, post_data_7)
            .await
            .unwrap();
        // Direct message from followed user sent to another user
        let post_data_8 = PostCreateData {
            content: "test post".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![user_1.id],
            ..Default::default()
        };
        let post_8 = create_post(db_client, &user_2.id, post_data_8)
            .await
            .unwrap();
        // Followers-only post from followed user
        let post_data_9 = PostCreateData {
            content: "followers only".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_9 = create_post(db_client, &user_2.id, post_data_9)
            .await
            .unwrap();
        // Subscribers-only post by followed user
        let post_data_10 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_10 = create_post(db_client, &user_2.id, post_data_10)
            .await
            .unwrap();
        // Subscribers-only post by subscription (without mention)
        let user_data_3 = UserCreateData {
            username: "subscription".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user_3 = create_user(db_client, user_data_3).await.unwrap();
        subscribe(db_client, &current_user.id, &user_3.id)
            .await
            .unwrap();
        let post_data_11 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_11 = create_post(db_client, &user_3.id, post_data_11)
            .await
            .unwrap();
        // Subscribers-only post by subscription (with mention)
        let post_data_12 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            mentions: vec![current_user.id],
            ..Default::default()
        };
        let post_12 = create_post(db_client, &user_3.id, post_data_12)
            .await
            .unwrap();
        // Repost from followed user if hiding reposts
        let user_data_4 = UserCreateData {
            username: "hide reposts".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user_4 = create_user(db_client, user_data_4).await.unwrap();
        follow(db_client, &current_user.id, &user_4.id)
            .await
            .unwrap();
        hide_reposts(db_client, &current_user.id, &user_4.id)
            .await
            .unwrap();
        let post_data_13 = PostCreateData {
            repost_of_id: Some(post_3.id),
            ..Default::default()
        };
        let post_13 = create_post(db_client, &user_4.id, post_data_13)
            .await
            .unwrap();

        let timeline = get_home_timeline(db_client, &current_user.id, None, 20)
            .await
            .unwrap();
        assert_eq!(timeline.len(), 7);
        assert!(timeline.iter().any(|post| post.id == post_1.id));
        assert!(timeline.iter().any(|post| post.id == post_2.id));
        assert!(!timeline.iter().any(|post| post.id == post_3.id));
        assert!(timeline.iter().any(|post| post.id == post_4.id));
        assert!(!timeline.iter().any(|post| post.id == post_5.id));
        assert!(timeline.iter().any(|post| post.id == post_6.id));
        assert!(timeline.iter().any(|post| post.id == post_7.id));
        assert!(!timeline.iter().any(|post| post.id == post_8.id));
        assert!(timeline.iter().any(|post| post.id == post_9.id));
        assert!(!timeline.iter().any(|post| post.id == post_10.id));
        assert!(!timeline.iter().any(|post| post.id == post_11.id));
        assert!(timeline.iter().any(|post| post.id == post_12.id));
        assert!(!timeline.iter().any(|post| post.id == post_13.id));
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_timeline_public() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        // Public post
        let post_data_1 = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post_1 = create_post(db_client, &user.id, post_data_1).await.unwrap();
        // Followers only post
        let post_data_2 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_2 = create_post(db_client, &user.id, post_data_2).await.unwrap();
        // Subscribers only post
        let post_data_3 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_3 = create_post(db_client, &user.id, post_data_3).await.unwrap();
        // Direct message
        let post_data_4 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let post_4 = create_post(db_client, &user.id, post_data_4).await.unwrap();
        // Reply
        let reply_data = PostCreateData {
            content: "my reply".to_string(),
            in_reply_to_id: Some(post_1.id),
            ..Default::default()
        };
        let reply = create_post(db_client, &user.id, reply_data).await.unwrap();
        // Repost
        let repost_data = PostCreateData {
            repost_of_id: Some(reply.id),
            ..Default::default()
        };
        let repost = create_post(db_client, &user.id, repost_data).await.unwrap();

        // Anonymous viewer
        let timeline = get_posts_by_author(db_client, &user.id, None, false, true, None, 10)
            .await
            .unwrap();
        assert_eq!(timeline.len(), 2);
        assert!(timeline.iter().any(|post| post.id == post_1.id));
        assert!(!timeline.iter().any(|post| post.id == post_2.id));
        assert!(!timeline.iter().any(|post| post.id == post_3.id));
        assert!(!timeline.iter().any(|post| post.id == post_4.id));
        assert!(!timeline.iter().any(|post| post.id == reply.id));
        assert!(timeline.iter().any(|post| post.id == repost.id));
    }

    #[tokio::test]
    #[serial]
    async fn test_get_thread() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let post_data_1 = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post_1 = create_post(db_client, &user.id, post_data_1).await.unwrap();
        let post_data_2 = PostCreateData {
            content: "my reply".to_string(),
            in_reply_to_id: Some(post_1.id),
            ..Default::default()
        };
        let post_2 = create_post(db_client, &user.id, post_data_2).await.unwrap();
        let thread = get_thread(db_client, &post_2.id, Some(&user.id))
            .await
            .unwrap();
        assert_eq!(thread[0].id, post_1.id);
        assert_eq!(thread[1].id, post_2.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_find_extraneous_posts() {
        let db_client = &mut create_test_database().await;
        let author_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let author = create_profile(db_client, author_data).await.unwrap();
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        create_post(db_client, &author.id, post_data).await.unwrap();
        let updated_before = Utc::now() - Duration::days(1);
        let result = find_extraneous_posts(db_client, &updated_before)
            .await
            .unwrap();
        assert!(result.is_empty());
    }
}
