use std::convert::TryFrom;

use chrono::Utc;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::attachments::types::DbMediaAttachment;
use crate::models::profiles::queries::update_post_count;
use super::types::{DbPost, Post, PostCreateData};

pub async fn get_posts(
    db_client: &impl GenericClient,
    current_user_id: &Uuid,
) -> Result<Vec<Post>, DatabaseError> {
    // Select posts from follows + own posts
    let rows = db_client.query(
        "
        SELECT
            post, actor_profile,
            ARRAY(
                SELECT media_attachment
                FROM media_attachment WHERE post_id = post.id
            ) AS attachments
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            post.author_id = $1
            OR EXISTS (
                SELECT 1 FROM relationship
                WHERE source_id = $1 AND target_id = post.author_id
            )
        ORDER BY post.created_at DESC
        ",
        &[&current_user_id],
    ).await?;
    let posts: Vec<Post> = rows.iter()
        .map(|row| Post::try_from(row))
        .collect::<Result<_, _>>()?;
    Ok(posts)
}

pub async fn get_posts_by_author(
    db_client: &impl GenericClient,
    account_id: &Uuid,
) -> Result<Vec<Post>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT
            post, actor_profile,
            ARRAY(
                SELECT media_attachment
                FROM media_attachment WHERE post_id = post.id
            ) AS attachments
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE
            post.author_id = $1
        ORDER BY post.created_at DESC
        ",
        &[&account_id],
    ).await?;
    let posts: Vec<Post> = rows.iter()
        .map(|row| Post::try_from(row))
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
        INSERT INTO post (id, author_id, content, created_at)
        VALUES ($1, $2, $3, $4)
        RETURNING post
        ",
        &[&post_id, &author_id, &data.content, &created_at],
    ).await?;
    let attachment_rows = transaction.query(
        "
        UPDATE media_attachment
        SET post_id = $1
        WHERE id = ANY($2)
        RETURNING media_attachment
        ",
        &[&post_id, &data.attachments],
    ).await?;
    let db_attachments: Vec<DbMediaAttachment> = attachment_rows.iter()
        .map(|row| -> Result<DbMediaAttachment, tokio_postgres::Error> {
            row.try_get("media_attachment")
        })
        .collect::<Result<_, _>>()?;
    let db_post: DbPost = post_row.try_get("post")?;
    let author = update_post_count(&transaction, &db_post.author_id, 1).await?;
    transaction.commit().await?;
    let post = Post {
        id: db_post.id,
        author: author,
        content: db_post.content,
        attachments: db_attachments,
        ipfs_cid: db_post.ipfs_cid,
        token_id: db_post.token_id,
        token_tx_id: db_post.token_tx_id,
        created_at: db_post.created_at,
    };
    Ok(post)
}

pub async fn get_post_by_id(
    db_client: &impl GenericClient,
    post_id: &Uuid,
) -> Result<Post, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT
            post, actor_profile,
            ARRAY(
                SELECT media_attachment
                FROM media_attachment WHERE post_id = post.id
            ) AS attachments
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.id = $1
        ",
        &[&post_id],
    ).await?;
    let post = match maybe_row {
        Some(row) => Post::try_from(&row)?,
        None => return Err(DatabaseError::NotFound("post")),
    };
    Ok(post)
}

pub async fn get_post_by_ipfs_cid(
    db_client: &impl GenericClient,
    ipfs_cid: &str,
) -> Result<Post, DatabaseError> {
    let result = db_client.query_opt(
        "
        SELECT
            post, actor_profile,
            ARRAY(
                SELECT media_attachment
                FROM media_attachment WHERE post_id = post.id
            ) AS attachments
        FROM post
        JOIN actor_profile ON post.author_id = actor_profile.id
        WHERE post.ipfs_cid = $1
        ",
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

pub async fn is_waiting_for_token(
    db_client: &impl GenericClient,
) -> Result<bool, DatabaseError> {
    let row = db_client.query_one(
        "
        SELECT count(post) > 0 AS is_waiting
        FROM post
        WHERE ipfs_cid IS NOT NULL AND token_id IS NULL
        ",
        &[],
    ).await?;
    let is_waiting: bool = row.try_get("is_waiting")?;
    Ok(is_waiting)
}
