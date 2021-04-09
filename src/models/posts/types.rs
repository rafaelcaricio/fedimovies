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
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct Post {
    pub id: Uuid,
    pub author: DbActorProfile,
    pub content: String,
    pub attachments: Vec<DbMediaAttachment>,
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<&Row> for Post {

    type Error = tokio_postgres::Error;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_post: DbPost = row.try_get("post")?;
        let db_profile: DbActorProfile = row.try_get("actor_profile")?;
        let db_attachments: Vec<DbMediaAttachment> = row.try_get("attachments")?;
        let post = Post {
            id: db_post.id,
            author: db_profile,
            content: db_post.content,
            attachments: db_attachments,
            ipfs_cid: db_post.ipfs_cid,
            token_id: db_post.token_id,
            token_tx_id: db_post.token_tx_id,
            created_at: db_post.created_at,
        };
        Ok(post)
    }
}

pub struct PostCreateData {
    pub content: String,
    pub attachments: Vec<Uuid>,
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
            attachments: vec![],
            created_at: None,
        };
        assert_eq!(post_data_1.validate().is_ok(), false);
    }

    #[test]
    fn test_trimming() {
        let mut post_data_2 = PostCreateData {
            content: "test ".to_string(),
            attachments: vec![],
            created_at: None,
        };
        assert_eq!(post_data_2.validate().is_ok(), true);
        assert_eq!(post_data_2.content.as_str(), "test");
    }
}
