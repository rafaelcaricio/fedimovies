use std::convert::TryFrom;

use chrono::Utc;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::attachments::types::DbMediaAttachment;
use crate::models::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use crate::models::notifications::queries::create_reply_notification;
use crate::models::profiles::queries::update_post_count;
use crate::models::profiles::types::DbActorProfile;
use super::types::{DbPost, Post, PostCreateData, Visibility};

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

pub async fn get_home_timeline(
    db_client: &impl GenericClient,
    current_user_id: &Uuid,
) -> Result<Vec<Post>, DatabaseError> {
    // Select posts from follows + own posts.
    // Exclude direct messages where current user is not mentioned.
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions}
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
                OR EXISTS (
                    SELECT 1 FROM mention
                    WHERE post_id = post.id AND profile_id = $1
                )
            )
        ORDER BY post.created_at DESC
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        visibility_public=i16::from(&Visibility::Public),
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&current_user_id],
    ).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts_by_author(
    db_client: &impl GenericClient,
    account_id: &Uuid,
    include_replies: bool,
    include_private: bool,
) -> Result<Vec<Post>, DatabaseError> {
    let mut condition = "post.author_id = $1".to_string();
    if !include_replies {
        condition.push_str(" AND post.in_reply_to_id IS NULL");
    };
    if !include_private {
        let only_public = format!(
            " AND visibility = {}",
            i16::from(&Visibility::Public),
        );
        condition.push_str(&only_public);
    };
    let statement = format!(
        "
        SELECT
            post, actor_profile,
            {related_attachments},
            {related_mentions}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE {condition}
        ORDER BY post.created_at DESC
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
        condition=condition,
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&account_id],
    ).await?;
    let posts: Vec<Post> = rows.iter()
        .map(Post::try_from)
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn create_post(
    db_client: &mut impl GenericClient,
    author_id: &Uuid,
    data: PostCreateData,
) -> Result<Post, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let post_id = uuid::Uuid::new_v4();
    let created_at = data.created_at.unwrap_or(Utc::now());
    let post_row = transaction.query_one(
        "
        INSERT INTO post (
            id, author_id, content,
            in_reply_to_id,
            visibility,
            object_id,
            created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING post
        ",
        &[
            &post_id,
            &author_id,
            &data.content,
            &data.in_reply_to_id,
            &data.visibility,
            &data.object_id,
            &created_at,
        ],
    ).await?;
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
    // Update counters
    let author = update_post_count(&transaction, &db_post.author_id, 1).await?;
    if let Some(in_reply_to_id) = &db_post.in_reply_to_id {
        update_reply_count(&transaction, in_reply_to_id, 1).await?;
        let in_reply_to = get_post_by_id(&transaction, in_reply_to_id).await?;
        if in_reply_to.author.is_local() {
            create_reply_notification(
                &transaction,
                &db_post.author_id,
                &in_reply_to.author.id,
                &db_post.id,
            ).await?;
        }
    }

    transaction.commit().await?;
    let post = Post::new(db_post, author, db_attachments, db_mentions);
    Ok(post)
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
            {related_mentions}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
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
) -> Result<Vec<Post>, DatabaseError> {
    // TODO: limit recursion depth
    let statement = format!(
        "
        WITH RECURSIVE
        ancestors (id, in_reply_to_id) AS (
            SELECT post.id, post.in_reply_to_id FROM post
            WHERE post.id = $1
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
            {related_mentions}
        FROM post
        JOIN context ON post.id = context.id
        JOIN actor_profile ON post.author_id = actor_profile.id
        ORDER BY context.path
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
    );
    let rows = db_client.query(
        statement.as_str(),
        &[&post_id],
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
            {related_mentions}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.object_id = $1
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
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
            {related_mentions}
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.ipfs_cid = $1
        ",
        related_attachments=RELATED_ATTACHMENTS,
        related_mentions=RELATED_MENTIONS,
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
    // TODO: create PostUpdateData type
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

pub async fn get_token_waitlist(
    db_client: &impl GenericClient,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT post.id
        FROM post
        WHERE ipfs_cid IS NOT NULL AND token_id IS NULL
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
    update_post_count(&transaction, &db_post.author_id, -1).await?;
    let orphaned_files = find_orphaned_files(&transaction, files).await?;
    let orphaned_ipfs_objects = find_orphaned_ipfs_objects(&transaction, ipfs_objects).await?;
    transaction.commit().await?;
    Ok(DeletionQueue {
        files: orphaned_files,
        ipfs_objects: orphaned_ipfs_objects,
    })
}
