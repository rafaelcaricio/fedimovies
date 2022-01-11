use serde::Serialize;
use uuid::Uuid;

use super::utils::get_ipfs_url;

// https://docs.opensea.io/docs/metadata-standards
#[derive(Serialize)]
pub struct PostMetadata {
    name: String,
    description: String,
    image: String,
    external_url: String,
}

impl PostMetadata {
    pub fn new(
        post_id: &Uuid,
        post_url: &str,
        content: &str,
        image_cid: &str,
    ) -> Self {
        Self {
            name: format!("Post {}", post_id),
            description: content.to_string(),
            image: get_ipfs_url(image_cid),
            external_url: post_url.to_string(),
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
            image_cid,
        );

        assert_eq!(post_metadata.name, format!("Post {}", post.id));
        assert_eq!(post_metadata.description, post.content);
        assert_eq!(post_metadata.image, format!("ipfs://{}", image_cid));
        assert_eq!(post_metadata.external_url, post_url);
    }
}
