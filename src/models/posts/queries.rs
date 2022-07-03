use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::database::query_macro::query;
use crate::errors::DatabaseError;
use crate::models::attachments::queries::set_attachment_ipfs_cid;
use crate::models::attachments::types::DbMediaAttachment;
use crate::models::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use crate::models::notifications::queries::{
    create_mention_notification,
    create_reply_notification,
    create_repost_notification,
};
use crate::models::profiles::queries::update_post_count;
use crate::models::profiles::types::DbActorProfile;
use crate::models::relationships::types::RelationshipType;
use crate::utils::id::new_uuid;
use super::types::{
    DbPost,
    Post,
    PostCreateData,
    PostUpdateData,
    Visibility,
};

pub async fn create_post(
    db_client: &mut impl GenericClient,
    author_id: &Uuid,
    data: PostCreateData,
) -> Result<Post, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let post_id = new_uuid();
    let created_at = data.created_at.unwrap_or(Utc::now());
    // Replying to reposts is not allowed
    // Reposting of other reposts or non-public posts is not allowed
    let insert_statement = format!(
        "
        INSERT INTO post (
            id, author_id, content,
            in_reply_to_id,
            repost_of_id,
            visibility,
            object_id,
            created_at
        )
        SELECT $1, $2, $3, $4, $5, $6, $7, $8
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
        visibility_public=i16::from(&Visibility::Public),
    );
    let maybe_post_row = transaction.query_opt(
        insert_statement.as_str(),
        &[
            &post_id,
            &author_id,
            &data.content,
            &data.in_reply_to_id,
            &data.repost_of_id,
            &data.visibility,
            &data.object_id,
            &created_at,
        ],
    ).await.map_err(catch_unique_violation("post"))?;
    let post_row = maybe_post_row.ok_or(DatabaseError::NotFound("post"))?;
    let db_post: DbPost = post_row.try_get("post")?;
    // Create links to attachments
    let attachments_rows = transaction.query(
        "
        UPDATE media_attachment
        SET post_id = $1
        WHERE owner_id = $2 AND id = ANY($3)
        RETURNING media_attachment
        ",
        &[&post_id, &author_id, &data.attachments],
    ).await?;
    if attachments_rows.len() != data.attachments.len() {
        // Some attachments were not found
        return Err(DatabaseError::NotFound("attachment"));
    }
    let db_attachments: Vec<DbMediaAttachment> = attachments_rows.iter()
        .map(|row| row.try_get("media_attachment"))
        .collect::<Result<_, _>>()?;
    // Create mentions
    let mentions_rows = transaction.query(
        "
        INSERT INTO mention (post_id, profile_id)
        SELECT $1, unnest($2::uuid[])
        RETURNING (
            SELECT actor_profile FROM actor_profile
            WHERE actor_profile.id = profile_id
        ) AS actor_profile
        ",
        &[&db_post.id, &data.mentions],
    ).await?;
    if mentions_rows.len() != data.mentions.len() {
        // Some profiles were not found
        return Err(DatabaseError::NotFound("profile"));
    };
    let db_mentions: Vec<DbActorProfile> = mentions_rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    // Create tags
    transaction.execute(
        "
        INSERT INTO tag (tag_name)
        SELECT unnest($1::text[])
        ON CONFLICT (tag_name) DO NOTHING
        ",
        &[&data.tags],
    ).await?;
    let tags_rows = transaction.query(
        "
        INSERT INTO post_tag (post_id, tag_id)
        SELECT $1, tag.id FROM tag WHERE tag_name = ANY($2)
        RETURNING (SELECT tag_name FROM tag WHERE tag.id = tag_id)
        ",
        &[&db_post.id, &data.tags],
    ).await?;
    if tags_rows.len() != data.tags.len() {
        return Err(DatabaseError::NotFound("tag"));
    };
    let db_tags: Vec<String> = tags_rows.iter()
        .map(|row| row.try_get("tag_name"))
        .collect::<Result<_, _>>()?;
    // Update counters
    let author = update_post_count(&transaction, &db_post.author_id, 1).await?;
    let mut notified_users = vec![];
    if let Some(in_reply_to_id) = &db_post.in_reply_to_id {
        update_reply_count(&transaction, in_reply_to_id, 1).await?;
        let in_reply_to_author = get_post_author(&transaction, in_reply_to_id).await?;
        if in_reply_to_author.is_local() &&
            in_reply_to_author.id != db_post.author_id
        {
            create_reply_notification(
                &transaction,
                &db_post.author_id,
                &in_reply_to_author.id,
                &db_post.id,
            ).await?;
            notified_users.push(in_reply_to_author.id);
        };
    }
    if let Some(repost_of_id) = &db_post.repost_of_id {
        update_repost_count(&transaction, repost_of_id, 1).await?;
        let repost_of_author = get_post_author(&transaction, repost_of_id).await?;
        if repost_of_author.is_local() &&
            repost_of_author.id != db_post.author_id &&
            !notified_users.contains(&repost_of_author.id)
        {
            create_repost_notification(
                &transaction,
                &db_post.author_id,
                &repost_of_author.id,
                repost_of_id,
            ).await?;
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
            create_mention_notification(
                &transaction,
                &db_post.author_id,
                &profile.id,
                &db_post.id,
            ).await?;
        };
    };

    transaction.commit().await?;
    let post = Post::new(db_post, author, db_attachments, db_mentions, db_tags)?;
    Ok(post)
}

pub const RELATED_ATTACHMENTS: &str =
    "ARRAY(
        SELECT media_attachment
        FROM media_attachment WHERE post_id = post.id
    ) AS attachments";

pub const RELATED_MENTIONS: &str =
    "ARRAY(
        SELECT actor_profile
        FROM mention
        JOIN actor_profile ON mention.profile_id = actor_profile.id
        WHERE post_id = post.id
    ) AS mentions";

pub const RELATED_TAGS: &str =
    "ARRAY(
        SELECT tag.tag_name FROM tag
        JOIN post_tag ON post_tag.tag_id = tag.id
        WHERE post_tag.post_id = post.id
    ) AS tags";

fn build_visibility_filter() -> String {
    format!(
        "(
            post.author_id = $current_user_id
            OR post.visibility = {visibility_public}
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
            OR post.visibility = {visibility_subscribers} AND EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = $current_user_id
                    AND target_id = post.author_id
                    AND relationship_type = {relationship_subscription}
            )
        )",
        visibility_public=i16::from(&Visibility::Public),
        visibility_followers=i16::from(&Visibility::Followers),
        visibility_subscribers=i16::from(&Visibility::Subscribers),
        relationship_follow=i16::from(&RelationshipType::Follow),
        relationship_subscription=i16::from(&RelationshipType::Subscription),
    )
}

pub async fn get_home_timeline(
    db_client: &impl GenericClient,
    current_user_id: &Uuid,
    max_post_id: Option<Uuid>,
    limit: i64,
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
            {related_tags}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            (
                post.author_id = $current_user_id
                OR (
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
            AND {visibility_filter}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        relationship_follow=i16::from(&RelationshipType::Follow),
        relationship_subscription=i16::from(&RelationshipType::Subscription),
        relationship_hide_reposts=i16::from(&RelationshipType::HideReposts),
        relationship_hide_replies=i16::from(&RelationshipType::HideReplies),
        visibility_filter=build_visibility_filter(),
    );
    let query = query!(
        &statement,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_local_timeline(
    db_client: &impl GenericClient,
    current_user_id: &Uuid,
    max_post_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Post>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            actor_profile.actor_json IS NULL
            AND post.visibility = {visibility_public}
            AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)
        ORDER BY post.id DESC
        LIMIT $limit
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        visibility_public=i16::from(&Visibility::Public),
    );
    let query = query!(
        &statement,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts(
    db_client: &impl GenericClient,
    posts_ids: Vec<Uuid>,
) -> Result<Vec<Post>, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = ANY($1)
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&posts_ids],
    ).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts_by_author(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
    current_user_id: Option<&Uuid>,
    include_replies: bool,
    include_reposts: bool,
    max_post_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Post>, DatabaseError> {
    let mut condition = format!(
        "post.author_id = $profile_id
        AND {visibility_filter}
        AND ($max_post_id::uuid IS NULL OR post.id < $max_post_id)",
        visibility_filter=build_visibility_filter(),
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
            {related_tags}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE {condition}
        ORDER BY post.created_at DESC
        LIMIT $limit
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        condition=condition,
    );
    let query = query!(
        &statement,
        profile_id=profile_id,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts_by_tag(
    db_client: &impl GenericClient,
    tag_name: &str,
    current_user_id: Option<&Uuid>,
    max_post_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Post>, DatabaseError> {
    let tag_name = tag_name.to_lowercase();
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags}
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
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        visibility_filter=build_visibility_filter(),
    );
    let query = query!(
        &statement,
        tag_name=tag_name,
        current_user_id=current_user_id,
        max_post_id=max_post_id,
        limit=limit,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_post_by_id(
    db_client: &impl GenericClient,
    post_id: &Uuid,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
    );
    let maybe_row = db_client.query_opt(
        statement.as_str(),
        &[&post_id],
    ).await?;
    let post = match maybe_row {
        Some(row) => Post::try_from(&row)?,
        None => return Err(DatabaseError::NotFound("post")),
    };
    Ok(post)
}

/// Given a post ID, finds all items in thread.
/// Results are sorted by tree path.
pub async fn get_thread(
    db_client: &impl GenericClient,
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
            {related_tags}
        FROM post
        JOIN thread ON post.id = thread.id
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE {visibility_filter}
        ORDER BY thread.path
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        visibility_filter=build_visibility_filter(),
    );
    let query = query!(
        &statement,
        post_id=post_id,
        current_user_id=current_user_id,
    )?;
    let rows = db_client.query(query.sql(), query.parameters()).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    if posts.is_empty() {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(posts)
}

pub async fn get_post_by_object_id(
    db_client: &impl GenericClient,
    object_id: &str,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.object_id = $1
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
    );
    let maybe_row = db_client.query_opt(
        statement.as_str(),
        &[&object_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let post = Post::try_from(&row)?;
    Ok(post)
}

pub async fn get_post_by_ipfs_cid(
    db_client: &impl GenericClient,
    ipfs_cid: &str,
) -> Result<Post, DatabaseError> {
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.ipfs_cid = $1
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
    );
    let result = db_client.query_opt(
        statement.as_str(),
        &[&ipfs_cid],
    ).await?;
    let post = match result {
        Some(row) => Post::try_from(&row)?,
        None => return Err(DatabaseError::NotFound("post")),
    };
    Ok(post)
}

pub async fn update_post(
    db_client: &impl GenericClient,
    post_id: &Uuid,
    post_data: PostUpdateData,
) -> Result<(), DatabaseError> {
    // Reposts and immutable posts can't be updated
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET
            content = $1,
            updated_at = $2
        WHERE id = $3
            AND repost_of_id IS NULL
            AND ipfs_cid IS NULL
        ",
        &[
            &post_data.content,
            &post_data.updated_at,
            &post_id,
        ],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(())
}

pub async fn update_reply_count(
    db_client: &impl GenericClient,
    post_id: &Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET reply_count = reply_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
        &[&change, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(())
}

pub async fn update_reaction_count(
    db_client: &impl GenericClient,
    post_id: &Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET reaction_count = reaction_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
        &[&change, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(())
}

pub async fn update_repost_count(
    db_client: &impl GenericClient,
    post_id: &Uuid,
    change: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET repost_count = repost_count + $1
        WHERE id = $2 AND repost_of_id IS NULL
        ",
        &[&change, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    }
    Ok(())
}

pub async fn set_post_ipfs_cid(
    db_client: &mut impl GenericClient,
    post_id: &Uuid,
    ipfs_cid: &str,
    attachments: Vec<(Uuid, String)>,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let updated_count = transaction.execute(
        "
        UPDATE post
        SET ipfs_cid = $1
        WHERE id = $2
            AND repost_of_id IS NULL
            AND ipfs_cid IS NULL
        ",
        &[&ipfs_cid, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    for (attachment_id, cid) in attachments {
        set_attachment_ipfs_cid(&transaction, &attachment_id, &cid).await?;
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn set_post_token_id(
    db_client: &impl GenericClient,
    post_id: &Uuid,
    token_id: i32,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET token_id = $1
        WHERE id = $2
            AND repost_of_id IS NULL
            AND token_id IS NULL
        ",
        &[&token_id, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn set_post_token_tx_id(
    db_client: &impl GenericClient,
    post_id: &Uuid,
    token_tx_id: &str,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET token_tx_id = $1
        WHERE id = $2
            AND repost_of_id IS NULL
        ",
        &[&token_tx_id, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(())
}

pub async fn get_post_author(
    db_client: &impl GenericClient,
    post_id: &Uuid,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT actor_profile
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
        ",
        &[&post_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("post"))?;
    let author: DbActorProfile = row.try_get("actor_profile")?;
    Ok(author)
}

/// Finds reposts of given posts and returns their IDs
pub async fn find_reposts_by_user(
    db_client: &impl GenericClient,
    user_id: &Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post.id
        FROM post
        WHERE post.author_id = $1 AND post.repost_of_id = ANY($2)
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let reposts: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(reposts)
}

/// Finds items reposted by user among given posts
pub async fn find_reposted_by_user(
    db_client: &impl GenericClient,
    user_id: &Uuid,
    posts_ids: &[Uuid],
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post.id
        FROM post
        WHERE post.id = ANY($2) AND EXISTS (
            SELECT 1 FROM post AS repost
            WHERE repost.author_id = $1 AND repost.repost_of_id = post.id
        )
        ",
        &[&user_id, &posts_ids],
    ).await?;
    let reposted: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(reposted)
}

pub async fn get_token_waitlist(
    db_client: &impl GenericClient,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post.id
        FROM post
        WHERE token_tx_id IS NOT NULL AND token_id IS NULL
        ",
        &[],
    ).await?;
    let waitlist: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(waitlist)
}

/// Finds all contexts (identified by top-level post)
/// created before the specified date
/// that do not contain local posts, reposts, mentions or reactions.
pub async fn find_extraneous_posts(
    db_client: &impl GenericClient,
    created_before: &DateTime<Utc>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        WITH RECURSIVE context (id, post_id) AS (
            SELECT post.id, post.id FROM post
            WHERE
                post.in_reply_to_id IS NULL
                AND post.repost_of_id IS NULL
                AND post.created_at < $1
            UNION
            SELECT context.id, post.id FROM post
            JOIN context ON (
                post.in_reply_to_id = context.post_id
                OR post.repost_of_id = context.post_id
            )
        )
        SELECT context_agg.id
        FROM (
            SELECT context.id, array_agg(context.post_id) AS posts
            FROM context
            GROUP BY context.id
        ) AS context_agg
        WHERE
            NOT EXISTS (
                SELECT 1
                FROM post
                JOIN actor_profile ON post.author_id = actor_profile.id
                WHERE
                    post.id = ANY(context_agg.posts)
                    AND actor_profile.actor_json IS NULL
            )
            AND NOT EXISTS (
                SELECT 1
                FROM mention
                JOIN actor_profile ON mention.profile_id = actor_profile.id
                WHERE
                    mention.post_id = ANY(context_agg.posts)
                    AND actor_profile.actor_json IS NULL
            )
            AND NOT EXISTS (
                SELECT 1
                FROM post_reaction
                JOIN actor_profile ON post_reaction.author_id = actor_profile.id
                WHERE
                    post_reaction.post_id = ANY(context_agg.posts)
                    AND actor_profile.actor_json IS NULL
            )
        ",
        &[&created_before],
    ).await?;
    let ids: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

/// Deletes post from database and returns collection of orphaned objects.
pub async fn delete_post(
    db_client: &mut impl GenericClient,
    post_id: &Uuid,
) -> Result<DeletionQueue, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Select all posts that will be deleted.
    // This includes given post, its descendants and reposts.
    let posts_rows = transaction.query(
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
    ).await?;
    let posts: Vec<Uuid> = posts_rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    // Get list of attached files
    let files_rows = transaction.query(
        "
        SELECT file_name
        FROM media_attachment WHERE post_id = ANY($1)
        ",
        &[&posts],
    ).await?;
    let files: Vec<String> = files_rows.iter()
        .map(|row| row.try_get("file_name"))
        .collect::<Result<_, _>>()?;
    // Get list of linked IPFS objects
    let ipfs_objects_rows = transaction.query(
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
    ).await?;
    let ipfs_objects: Vec<String> = ipfs_objects_rows.iter()
        .map(|row| row.try_get("ipfs_cid"))
        .collect::<Result<_, _>>()?;
    // Update post counters
    transaction.execute(
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
    ).await?;
    // Delete post
    let maybe_post_row = transaction.query_opt(
        "
        DELETE FROM post WHERE id = $1
        RETURNING post
        ",
        &[&post_id],
    ).await?;
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


#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::profiles::queries::create_profile;
    use crate::models::profiles::types::ProfileCreateData;
    use crate::models::relationships::queries::{
        follow,
        hide_reposts,
        subscribe,
    };
    use crate::models::users::queries::create_user;
    use crate::models::users::types::UserCreateData;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_post() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let post_data = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, &profile.id, post_data).await.unwrap();
        assert_eq!(post.content, "test post");
        assert_eq!(post.author.id, profile.id);
        assert_eq!(post.updated_at, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_post() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
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
        };
        update_post(db_client, &post.id, post_data).await.unwrap();
        let post = get_post_by_id(db_client, &post.id).await.unwrap();
        assert_eq!(post.content, "test update");
        assert_eq!(post.updated_at.is_some(), true);
    }

    #[tokio::test]
    #[serial]
    async fn test_home_timeline() {
        let db_client = &mut create_test_database().await;
        let current_user_data = UserCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let current_user = create_user(db_client, current_user_data).await.unwrap();
        // Current user's post
        let post_data_1 = PostCreateData {
            content: "my post".to_string(),
            ..Default::default()
        };
        let post_1 = create_post(db_client, &current_user.id, post_data_1).await.unwrap();
        // Current user's direct message
        let post_data_2 = PostCreateData {
            content: "my post".to_string(),
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let post_2 = create_post(db_client, &current_user.id, post_data_2).await.unwrap();
        // Another user's public post
        let user_data_1 = UserCreateData {
            username: "another-user".to_string(),
            ..Default::default()
        };
        let user_1 = create_user(db_client, user_data_1).await.unwrap();
        let post_data_3 = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post_3 = create_post(db_client, &user_1.id, post_data_3).await.unwrap();
        // Direct message from another user to current user
        let post_data_4 = PostCreateData {
            content: "test post".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![current_user.id],
            ..Default::default()
        };
        let post_4 = create_post(db_client, &user_1.id, post_data_4).await.unwrap();
        // Followers-only post from another user
        let post_data_5 = PostCreateData {
            content: "followers only".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_5 = create_post(db_client, &user_1.id, post_data_5).await.unwrap();
        // Followed user's public post
        let user_data_2 = UserCreateData {
            username: "followed".to_string(),
            ..Default::default()
        };
        let user_2 = create_user(db_client, user_data_2).await.unwrap();
        follow(db_client, &current_user.id, &user_2.id).await.unwrap();
        let post_data_6 = PostCreateData {
            content: "test post".to_string(),
            ..Default::default()
        };
        let post_6 = create_post(db_client, &user_2.id, post_data_6).await.unwrap();
        // Followed user's repost
        let post_data_7 = PostCreateData {
            repost_of_id: Some(post_3.id),
            ..Default::default()
        };
        let post_7 = create_post(db_client, &user_2.id, post_data_7).await.unwrap();
        // Direct message from followed user sent to another user
        let post_data_8 = PostCreateData {
            content: "test post".to_string(),
            visibility: Visibility::Direct,
            mentions: vec![user_1.id],
            ..Default::default()
        };
        let post_8 = create_post(db_client, &user_2.id, post_data_8).await.unwrap();
        // Followers-only post from followed user
        let post_data_9 = PostCreateData {
            content: "followers only".to_string(),
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let post_9 = create_post(db_client, &user_2.id, post_data_9).await.unwrap();
        // Subscribers-only post by followed user
        let post_data_10 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_10 = create_post(db_client, &user_2.id, post_data_10).await.unwrap();
        // Subscribers-only post by subscription
        let user_data_3 = UserCreateData {
            username: "subscription".to_string(),
            ..Default::default()
        };
        let user_3 = create_user(db_client, user_data_3).await.unwrap();
        subscribe(db_client, &current_user.id, &user_3.id).await.unwrap();
        let post_data_11 = PostCreateData {
            content: "subscribers only".to_string(),
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        let post_11 = create_post(db_client, &user_3.id, post_data_11).await.unwrap();
        // Repost from followed user if hiding reposts
        let user_data_4 = UserCreateData {
            username: "hide reposts".to_string(),
            ..Default::default()
        };
        let user_4 = create_user(db_client, user_data_4).await.unwrap();
        follow(db_client, &current_user.id, &user_4.id).await.unwrap();
        hide_reposts(db_client, &current_user.id, &user_4.id).await.unwrap();
        let post_data_12 = PostCreateData {
            repost_of_id: Some(post_3.id),
            ..Default::default()
        };
        let post_12 = create_post(db_client, &user_4.id, post_data_12).await.unwrap();

        let timeline = get_home_timeline(db_client, &current_user.id, None, 20).await.unwrap();
        assert_eq!(timeline.len(), 7);
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_3.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_4.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_5.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_6.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_7.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_8.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_9.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_10.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_11.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_12.id), false);
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_timeline_public() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
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
            in_reply_to_id: Some(post_1.id.clone()),
            ..Default::default()
        };
        let reply = create_post(db_client, &user.id, reply_data).await.unwrap();
        // Repost
        let repost_data = PostCreateData {
            repost_of_id: Some(reply.id.clone()),
            ..Default::default()
        };
        let repost = create_post(db_client, &user.id, repost_data).await.unwrap();

        // Anonymous viewer
        let timeline = get_posts_by_author(
            db_client, &user.id, None, false, true, None, 10
        ).await.unwrap();
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.iter().any(|post| post.id == post_1.id), true);
        assert_eq!(timeline.iter().any(|post| post.id == post_2.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_3.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == post_4.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == reply.id), false);
        assert_eq!(timeline.iter().any(|post| post.id == repost.id), true);
    }
}
