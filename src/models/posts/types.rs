use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::errors::ValidationError;
use crate::models::attachments::types::DbMediaAttachment;
use crate::models::profiles::types::DbActorProfile;
use crate::utils::html::clean_html;

#[derive(FromSql)]
#[postgres(name = "post")]
pub struct DbPost {
    pub id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub object_id: Option<String>,
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

// List of user's actions
pub struct PostActions {
    pub favourited: bool,
}

pub struct Post {
    pub id: Uuid,
    pub author: DbActorProfile,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub attachments: Vec<DbMediaAttachment>,
    pub object_id: Option<String>,
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub actions: Option<PostActions>,
}

impl Post {
    pub fn new(
        db_post: DbPost,
        db_author: DbActorProfile,
        db_attachments: Vec<DbMediaAttachment>,
    ) -> Self {
        assert_eq!(db_post.author_id, db_author.id);
        Self {
            id: db_post.id,
            author: db_author,
            content: db_post.content,
            in_reply_to_id: db_post.in_reply_to_id,
            reply_count: db_post.reply_count,
            reaction_count: db_post.reaction_count,
            attachments: db_attachments,
            object_id: db_post.object_id,
            ipfs_cid: db_post.ipfs_cid,
            token_id: db_post.token_id,
            token_tx_id: db_post.token_tx_id,
            created_at: db_post.created_at,
            actions: None,
        }
    }
}

#[cfg(test)]
impl Default for Post {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            author: Default::default(),
            content: "".to_string(),
            in_reply_to_id: None,
            reply_count: 0,
            reaction_count: 0,
            attachments: vec![],
            object_id: None,
            ipfs_cid: None,
            token_id: None,
            token_tx_id: None,
            created_at: Utc::now(),
            actions: None,
        }
    }
}

impl TryFrom<&Row> for Post {

    type Error = tokio_postgres::Error;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_post: DbPost = row.try_get("post")?;
        let db_profile: DbActorProfile = row.try_get("actor_profile")?;
        let db_attachments: Vec<DbMediaAttachment> = row.try_get("attachments")?;
        let post = Self::new(db_post, db_profile, db_attachments);
        Ok(post)
    }
}

pub struct PostCreateData {
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub attachments: Vec<Uuid>,
    pub object_id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

impl PostCreateData {
    /// Validate and clean post data.
    pub fn validate(&mut self) -> Result<(), ValidationError> {
        let content_safe = clean_html(&self.content);
        let content_trimmed = content_safe.trim();
        if content_trimmed == "" {
            return Err(ValidationError("post can not be empty"));
        }
        self.content = content_trimmed.to_string();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_post_data() {
        let mut post_data_1 = PostCreateData {
            content: "  ".to_string(),
            in_reply_to_id: None,
            attachments: vec![],
            object_id: None,
            created_at: None,
        };
        assert_eq!(post_data_1.validate().is_ok(), false);
    }

    #[test]
    fn test_trimming() {
        let mut post_data_2 = PostCreateData {
            content: "test ".to_string(),
            in_reply_to_id: None,
            attachments: vec![],
            object_id: None,
            created_at: None,
        };
        assert_eq!(post_data_2.validate().is_ok(), true);
        assert_eq!(post_data_2.content.as_str(), "test");
    }
}
