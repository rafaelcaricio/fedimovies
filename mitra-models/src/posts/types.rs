use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::attachments::types::DbMediaAttachment;
use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseError,
    DatabaseTypeError,
};
use crate::emojis::types::DbEmoji;
use crate::profiles::types::DbActorProfile;

#[derive(Clone, Debug, PartialEq)]
pub enum Visibility {
    Public,
    Direct,
    Followers,
    Subscribers,
}

impl Default for Visibility {
    fn default() -> Self { Self::Public }
}

impl From<&Visibility> for i16 {
    fn from(value: &Visibility) -> i16 {
        match value {
            Visibility::Public => 1,
            Visibility::Direct => 2,
            Visibility::Followers => 3,
            Visibility::Subscribers => 4,
        }
    }
}

impl TryFrom<i16> for Visibility {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let visibility = match value {
            1 => Self::Public,
            2 => Self::Direct,
            3 => Self::Followers,
            4 => Self::Subscribers,
            _ => return Err(DatabaseTypeError),
        };
        Ok(visibility)
    }
}

int_enum_from_sql!(Visibility);
int_enum_to_sql!(Visibility);

#[derive(FromSql)]
#[postgres(name = "post")]
pub struct DbPost {
    pub id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub repost_of_id: Option<Uuid>,
    pub visibility: Visibility,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub repost_count: i32,
    pub object_id: Option<String>,
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>, // edited at
}

// List of user's actions
#[derive(Clone)]
pub struct PostActions {
    pub favourited: bool,
    pub reposted: bool,
}

#[derive(Clone)]
pub struct Post {
    pub id: Uuid,
    pub author: DbActorProfile,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub repost_of_id: Option<Uuid>,
    pub visibility: Visibility,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub repost_count: i32,
    pub attachments: Vec<DbMediaAttachment>,
    pub mentions: Vec<DbActorProfile>,
    pub tags: Vec<String>,
    pub links: Vec<Uuid>,
    pub emojis: Vec<DbEmoji>,
    pub object_id: Option<String>,
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,

    // These fields are not populated automatically
    // by functions in posts::queries module
    pub actions: Option<PostActions>,
    pub in_reply_to: Option<Box<Post>>,
    pub repost_of: Option<Box<Post>>,
    pub linked: Vec<Post>,
}

impl Post {
    pub fn new(
        db_post: DbPost,
        db_author: DbActorProfile,
        db_attachments: Vec<DbMediaAttachment>,
        db_mentions: Vec<DbActorProfile>,
        db_tags: Vec<String>,
        db_links: Vec<Uuid>,
        db_emojis: Vec<DbEmoji>,
    ) -> Result<Self, DatabaseTypeError> {
        // Consistency checks
        if db_post.author_id != db_author.id {
            return Err(DatabaseTypeError);
        };
        if db_author.is_local() != db_post.object_id.is_none() {
            return Err(DatabaseTypeError);
        };
        if db_post.repost_of_id.is_some() && (
            db_post.content.len() != 0 ||
            db_post.in_reply_to_id.is_some() ||
            db_post.ipfs_cid.is_some() ||
            db_post.token_id.is_some() ||
            db_post.token_tx_id.is_some() ||
            !db_attachments.is_empty() ||
            !db_mentions.is_empty() ||
            !db_tags.is_empty() ||
            !db_links.is_empty() ||
            !db_emojis.is_empty()
        ) {
            return Err(DatabaseTypeError);
        };
        let post = Self {
            id: db_post.id,
            author: db_author,
            content: db_post.content,
            in_reply_to_id: db_post.in_reply_to_id,
            repost_of_id: db_post.repost_of_id,
            visibility: db_post.visibility,
            reply_count: db_post.reply_count,
            reaction_count: db_post.reaction_count,
            repost_count: db_post.repost_count,
            attachments: db_attachments,
            mentions: db_mentions,
            tags: db_tags,
            links: db_links,
            emojis: db_emojis,
            object_id: db_post.object_id,
            ipfs_cid: db_post.ipfs_cid,
            token_id: db_post.token_id,
            token_tx_id: db_post.token_tx_id,
            created_at: db_post.created_at,
            updated_at: db_post.updated_at,
            actions: None,
            in_reply_to: None,
            repost_of: None,
            linked: vec![],
        };
        Ok(post)
    }

    pub fn is_local(&self) -> bool {
        self.author.is_local()
    }

    pub fn is_public(&self) -> bool {
        matches!(self.visibility, Visibility::Public)
    }
}

#[cfg(feature = "test-utils")]
impl Default for Post {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            author: Default::default(),
            content: "".to_string(),
            in_reply_to_id: None,
            repost_of_id: None,
            visibility: Visibility::Public,
            reply_count: 0,
            reaction_count: 0,
            repost_count: 0,
            attachments: vec![],
            mentions: vec![],
            tags: vec![],
            links: vec![],
            emojis: vec![],
            object_id: None,
            ipfs_cid: None,
            token_id: None,
            token_tx_id: None,
            created_at: Utc::now(),
            updated_at: None,
            actions: None,
            in_reply_to: None,
            repost_of: None,
            linked: vec![],
        }
    }
}

impl TryFrom<&Row> for Post {

    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_post: DbPost = row.try_get("post")?;
        let db_profile: DbActorProfile = row.try_get("actor_profile")?;
        let db_attachments: Vec<DbMediaAttachment> = row.try_get("attachments")?;
        let db_mentions: Vec<DbActorProfile> = row.try_get("mentions")?;
        let db_tags: Vec<String> = row.try_get("tags")?;
        let db_links: Vec<Uuid> = row.try_get("links")?;
        let db_emojis: Vec<DbEmoji> = row.try_get("emojis")?;
        let post = Self::new(
            db_post,
            db_profile,
            db_attachments,
            db_mentions,
            db_tags,
            db_links,
            db_emojis,
        )?;
        Ok(post)
    }
}

#[derive(Default)]
pub struct PostCreateData {
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub repost_of_id: Option<Uuid>,
    pub visibility: Visibility,
    pub attachments: Vec<Uuid>,
    pub mentions: Vec<Uuid>,
    pub tags: Vec<String>,
    pub links: Vec<Uuid>,
    pub emojis: Vec<Uuid>,
    pub object_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl PostCreateData {
    pub fn repost(
        repost_of_id: Uuid,
        object_id: Option<String>,
    ) -> Self {
        Self {
            repost_of_id: Some(repost_of_id),
            object_id: object_id,
            created_at: Utc::now(),
            ..Default::default()
        }
    }
}

#[cfg_attr(test, derive(Default))]
pub struct PostUpdateData {
    pub content: String,
    pub attachments: Vec<Uuid>,
    pub mentions: Vec<Uuid>,
    pub tags: Vec<String>,
    pub links: Vec<Uuid>,
    pub emojis: Vec<Uuid>,
    pub updated_at: DateTime<Utc>,
}
