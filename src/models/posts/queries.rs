use std::convert::TryFrom;

use chrono::Utc;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
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
use crate::utils::id::new_uuid;
use super::types::{DbPost, Post, PostCreateData, Visibility};

pub async fn create_post(
    db_client: &mut impl GenericClient,
    author_id: &Uuid,
    data: PostCreateData,
) -> Result<Post, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let post_id = new_uuid();
    let created_at = data.created_at.unwrap_or(Utc::now());
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
        WHERE NOT EXISTS (
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

pub async fn get_home_timeline(
    db_client: &impl GenericClient,
    current_user_id: &Uuid,
    max_post_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Post>, DatabaseError> {
    // Select posts from follows + own posts.
    // Exclude direct messages where current user is not mentioned.
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
                post.author_id = $1
                OR EXISTS (
                    SELECT 1 FROM relationship
                    WHERE source_id = $1 AND target_id = post.author_id
                )
            )
            AND (
                post.visibility = {visibility_public}
                OR post.author_id = $1
                OR EXISTS (
                    SELECT 1 FROM mention
                    WHERE post_id = post.id AND profile_id = $1
                )
            )
            AND ($2::uuid IS NULL OR post.id < $2)
        ORDER BY post.id DESC
        LIMIT $3
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        visibility_public=i16::from(&Visibility::Public),
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&current_user_id, &max_post_id, &limit],
    ).await?;
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
    max_post_id: Option<Uuid>,
    limit: i64,
) -> Result<Vec<Post>, DatabaseError> {
    let mut condition = "post.author_id = $1
        AND ($2::uuid IS NULL OR post.id < $2)".to_string();
    if !include_replies {
        condition.push_str(" AND post.in_reply_to_id IS NULL");
    };
    let visibility_filter = format!(
        " AND (
            post.visibility = {visibility_public}
            OR $4::uuid IS NULL
            OR post.author_id = $4
            OR EXISTS (
                SELECT 1 FROM mention
                WHERE post_id = post.id AND profile_id = $4
            )
        )",
        visibility_public=i16::from(&Visibility::Public),
    );
    condition.push_str(&visibility_filter);
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
        LIMIT $3
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        condition=condition,
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&profile_id, &max_post_id, &limit, &current_user_id],
    ).await?;
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
                post.visibility = {visibility_public}
                OR $4::uuid IS NULL
                OR post.author_id = $4
                OR EXISTS (
                    SELECT 1 FROM mention
                    WHERE post_id = post.id AND profile_id = $4
                )
            )
            AND EXISTS (
                SELECT 1 FROM post_tag JOIN tag ON post_tag.tag_id = tag.id
                WHERE post_tag.post_id = post.id AND tag.tag_name = $1
            )
            AND ($2::uuid IS NULL OR post.id < $2)
        ORDER BY post.id DESC
        LIMIT $3
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        visibility_public=i16::from(&Visibility::Public),
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&tag_name.to_lowercase(), &max_post_id, &limit, &current_user_id],
    ).await?;
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
    let condition = format!(
        "
        post.visibility = {visibility_public}
        OR $2::uuid IS NULL
        OR post.author_id = $2
        OR EXISTS (
            SELECT 1 FROM mention
            WHERE post_id = post.id AND profile_id = $2
        )
        ",
        visibility_public=i16::from(&Visibility::Public),
    );
    // TODO: limit recursion depth
    let statement = format!(
        "
        WITH RECURSIVE
        ancestors (id, in_reply_to_id) AS (
            SELECT post.id, post.in_reply_to_id FROM post
            WHERE post.id = $1 AND ({condition})
            UNION ALL
            SELECT post.id, post.in_reply_to_id FROM post
            JOIN ancestors ON post.id = ancestors.in_reply_to_id
        ),
        context (id, path) AS (
            SELECT ancestors.id, ARRAY[ancestors.id] FROM ancestors
            WHERE ancestors.in_reply_to_id IS NULL
            UNION
            SELECT post.id, array_append(context.path, post.id) FROM post
            JOIN context ON post.in_reply_to_id = context.id
        )
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions},
            {related_tags}
        FROM post
        JOIN context ON post.id = context.id
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE {condition}
        ORDER BY context.path
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        related_tags=RELATED_TAGS,
        condition=condition,
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&post_id, &current_user_id],
    ).await?;
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
    post: &Post,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE post
        SET
            content = $1,
            ipfs_cid = $2,
            token_id = $3,
            token_tx_id = $4
        WHERE id = $5
        ",
        &[
            &post.content,
            &post.ipfs_cid,
            &post.token_id,
            &post.token_tx_id,
            &post.id,
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
        WHERE id = $2
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
        WHERE id = $2
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
        WHERE id = $2
        ",
        &[&change, &post_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("post"));
    }
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

/// Deletes post from database and returns collection of orphaned objects.
pub async fn delete_post(
    db_client: &mut impl GenericClient,
    post_id: &Uuid,
) -> Result<DeletionQueue, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Get list of attached files
    let files_rows = transaction.query(
        "
        SELECT file_name
        FROM media_attachment WHERE post_id = $1
        ",
        &[&post_id],
    ).await?;
    let files: Vec<String> = files_rows.iter()
        .map(|row| row.try_get("file_name"))
        .collect::<Result<_, _>>()?;
    // Get list of linked IPFS objects
    let ipfs_objects_rows = transaction.query(
        "
        SELECT ipfs_cid
        FROM media_attachment
        WHERE post_id = $1 AND ipfs_cid IS NOT NULL
        UNION ALL
        SELECT ipfs_cid
        FROM post
        WHERE id = $1 AND ipfs_cid IS NOT NULL
        ",
        &[&post_id],
    ).await?;
    let ipfs_objects: Vec<String> = ipfs_objects_rows.iter()
        .map(|row| row.try_get("ipfs_cid"))
        .collect::<Result<_, _>>()?;
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
    update_post_count(&transaction, &db_post.author_id, -1).await?;
    let orphaned_files = find_orphaned_files(&transaction, files).await?;
    let orphaned_ipfs_objects = find_orphaned_ipfs_objects(&transaction, ipfs_objects).await?;
    transaction.commit().await?;
    Ok(DeletionQueue {
        files: orphaned_files,
        ipfs_objects: orphaned_ipfs_objects,
    })
}
