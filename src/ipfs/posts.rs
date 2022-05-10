use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use super::utils::get_ipfs_url;

const IPFS_LOGO: &str = "bafybeihc4hti5ix4ds2tefhy35qd4c7n5as5cazdmksrxj7ipvcxm64h54";

/// ERC-721 custom attribute as defined by OpenSea guidelines.
#[derive(Serialize)]
struct Attribute {
    trait_type: String,
    value: Value,
    display_type: String,
}

/// JSON representation of a post. Compatible with ERC-721 metadata standard.
/// https://docs.opensea.io/docs/metadata-standards
#[derive(Serialize)]
pub struct PostMetadata {
    // Fields required by ERC-721
    name: String,
    description: String,
    image: String,
    // OpenSea custom fields
    external_url: String,
    attributes: Vec<Attribute>,
}

impl PostMetadata {
    pub fn new(
        post_id: &Uuid,
        post_url: &str,
        content: &str,
        created_at: &DateTime<Utc>,
        image_cid: Option<&str>,
    ) -> Self {
        // Use IPFS logo if there's no image
        let image_cid = image_cid.unwrap_or(IPFS_LOGO);
        let created_at_attr = Attribute {
            trait_type: "Created at".to_string(),
            value: json!(created_at.timestamp()),
            display_type: "date".to_string(),
        };
        Self {
            name: format!("Post {}", post_id),
            description: content.to_string(),
            image: get_ipfs_url(image_cid),
            external_url: post_url.to_string(),
            attributes: vec![created_at_attr],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::models::posts::types::Post;
    use super::*;

    #[test]
    fn test_create_post_metadata() {
        let post = Post {
            content: "test".to_string(),
            ..Default::default()
        };
        let post_url = "https://example.com/objects/1";
        let image_cid = "bafybeihc4hti5ix4ds2tefhy35qd4c7n5as5cazdmksrxj7ipvcxm64h54";
        let post_metadata = PostMetadata::new(
            &post.id,
            post_url,
            &post.content,
            &post.created_at,
            Some(image_cid),
        );

        assert_eq!(post_metadata.name, format!("Post {}", post.id));
        assert_eq!(post_metadata.description, post.content);
        assert_eq!(post_metadata.image, format!("ipfs://{}", image_cid));
        assert_eq!(post_metadata.external_url, post_url);
        let created_at_attr = &post_metadata.attributes[0];
        assert_eq!(created_at_attr.display_type, "date");
        assert_eq!(created_at_attr.value.as_i64().is_some(), true);
    }
}
