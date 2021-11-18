use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use postgres_protocol::types::{int2_from_sql, int2_to_sql};
use postgres_types::{
    FromSql, ToSql, IsNull, Type,
    accepts, to_sql_checked,
    private::BytesMut,
};
use tokio_postgres::Row;
use uuid::Uuid;

use crate::errors::{ConversionError, ValidationError};
use crate::models::attachments::types::DbMediaAttachment;
use crate::models::profiles::types::DbActorProfile;
use crate::utils::html::clean_html;

#[derive(Debug)]
pub enum Visibility {
    Public,
    Direct,
}

impl From<&Visibility> for i16 {
    fn from(value: &Visibility) -> i16 {
        match value {
            Visibility::Public => 1,
            Visibility::Direct => 2,
        }
    }
}

impl TryFrom<i16> for Visibility {
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let visibility = match value {
            1 => Self::Public,
            2 => Self::Direct,
            _ => return Err(ConversionError),
        };
        Ok(visibility)
    }
}

type SqlError = Box<dyn std::error::Error + Sync + Send>;

impl<'a> FromSql<'a> for Visibility {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Visibility, SqlError> {
        let int_value = int2_from_sql(raw)?;
        let visibility = Visibility::try_from(int_value)?;
        Ok(visibility)
    }

    accepts!(INT2);
}

impl ToSql for Visibility {
    fn to_sql(&self, _: &Type, out: &mut BytesMut) -> Result<IsNull, SqlError> {
        let int_value: i16 = self.into();
        int2_to_sql(int_value, out);
        Ok(IsNull::No)
    }

    accepts!(INT2);
    to_sql_checked!();
}

#[derive(FromSql)]
#[postgres(name = "post")]
pub struct DbPost {
    pub id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub visibility: Visibility,
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
    pub visibility: Visibility,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub attachments: Vec<DbMediaAttachment>,
    pub mentions: Vec<DbActorProfile>,
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
        db_mentions: Vec<DbActorProfile>,
    ) -> Self {
        assert_eq!(db_post.author_id, db_author.id);
        Self {
            id: db_post.id,
            author: db_author,
            content: db_post.content,
            in_reply_to_id: db_post.in_reply_to_id,
            visibility: db_post.visibility,
            reply_count: db_post.reply_count,
            reaction_count: db_post.reaction_count,
            attachments: db_attachments,
            mentions: db_mentions,
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
            visibility: Visibility::Public,
            reply_count: 0,
            reaction_count: 0,
            attachments: vec![],
            mentions: vec![],
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
        let db_mentions: Vec<DbActorProfile> = row.try_get("mentions")?;
        let post = Self::new(db_post, db_profile, db_attachments, db_mentions);
        Ok(post)
    }
}

pub struct PostCreateData {
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub visibility: Visibility,
    pub attachments: Vec<Uuid>,
    pub mentions: Vec<Uuid>,
    pub object_id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

impl PostCreateData {
    /// Validate and clean post data.
    pub fn validate(&mut self) -> Result<(), ValidationError> {
        let content_safe = clean_html(&self.content);
        let content_trimmed = content_safe.trim();
        if content_trimmed.is_empty() {
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
            visibility: Visibility::Public,
            attachments: vec![],
            mentions: vec![],
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
            visibility: Visibility::Public,
            attachments: vec![],
            mentions: vec![],
            object_id: None,
            created_at: None,
        };
        assert_eq!(post_data_2.validate().is_ok(), true);
        assert_eq!(post_data_2.content.as_str(), "test");
    }
}
