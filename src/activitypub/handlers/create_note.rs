use std::collections::HashMap;
use std::path::Path;

use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::activity::{Attachment, Object};
use crate::activitypub::constants::AP_PUBLIC;
use crate::activitypub::fetcher::fetchers::fetch_file;
use crate::activitypub::fetcher::helpers::{
    get_or_import_profile_by_actor_id,
    import_profile_by_actor_address,
    ImportError,
};
use crate::activitypub::receiver::{
    parse_actor_id,
    parse_array,
    parse_object_id,
    parse_property_value,
};
use crate::activitypub::vocabulary::{DOCUMENT, HASHTAG, IMAGE, MENTION, NOTE};
use crate::config::Instance;
use crate::errors::{DatabaseError, ValidationError};
use crate::models::attachments::queries::create_attachment;
use crate::models::posts::mentions::mention_to_address;
use crate::models::posts::queries::{
    create_post,
    get_post_by_id,
    get_post_by_object_id,
};
use crate::models::posts::tags::normalize_tag;
use crate::models::posts::types::{Post, PostCreateData, Visibility};
use crate::models::profiles::queries::get_profile_by_acct;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::queries::get_user_by_name;
use crate::utils::html::clean_html;

fn get_note_author_id(object: &Object) -> Result<String, ValidationError> {
    let attributed_to = object.attributed_to.as_ref()
        .ok_or(ValidationError("unattributed note"))?;
    let author_id = parse_array(attributed_to)
        .map_err(|_| ValidationError("invalid attributedTo property"))?
        .get(0)
        .ok_or(ValidationError("invalid attributedTo property"))?
        .to_string();
    Ok(author_id)
}

const CONTENT_MAX_SIZE: usize = 100000;

pub fn get_note_content(object: &Object) -> Result<String, ValidationError> {
    let content = object.content.as_ref()
        // Lemmy pages and PeerTube videos have "name" property
        .or(object.name.as_ref())
        .ok_or(ValidationError("no content"))?;
    if content.len() > CONTENT_MAX_SIZE {
        return Err(ValidationError("content is too long"));
    };
    let content_safe = clean_html(content);
    Ok(content_safe)
}

fn get_note_visibility(
    author: &DbActorProfile,
    primary_audience: Vec<String>,
    secondary_audience: Vec<String>,
) -> Visibility {
    if primary_audience.contains(&AP_PUBLIC.to_string()) ||
            secondary_audience.contains(&AP_PUBLIC.to_string()) {
        Visibility::Public
    } else {
        let maybe_followers = author.actor_json.as_ref()
            .and_then(|actor| actor.followers.as_ref());
        if let Some(followers) = maybe_followers {
            if primary_audience.contains(followers) ||
                    secondary_audience.contains(followers) {
                Visibility::Followers
            } else {
                Visibility::Direct
            }
        } else {
            Visibility::Direct
        }
    }
}

pub async fn handle_note(
    db_client: &mut impl GenericClient,
    instance: &Instance,
    media_dir: &Path,
    object: Object,
    redirects: &HashMap<String, String>,
) -> Result<Post, ImportError> {
    if object.object_type != NOTE {
        // Could be Page (in Lemmy) or some other type
        log::warn!("processing object of type {}", object.object_type);
    };

    let author_id = get_note_author_id(&object)?;
    let author = get_or_import_profile_by_actor_id(
        db_client,
        instance,
        media_dir,
        &author_id,
    ).await?;
    let content = get_note_content(&object)?;

    let mut attachments: Vec<Uuid> = Vec::new();
    if let Some(value) = object.attachment {
        let list: Vec<Attachment> = parse_property_value(&value)
            .map_err(|_| ValidationError("invalid attachment property"))?;
        let mut downloaded = vec![];
        for attachment in list {
            if attachment.attachment_type != DOCUMENT &&
                attachment.attachment_type != IMAGE
            {
                log::warn!(
                    "skipping attachment of type {}",
                    attachment.attachment_type,
                );
                continue;
            };
            let attachment_url = attachment.url
                .ok_or(ValidationError("attachment URL is missing"))?;
            let (file_name, media_type) = fetch_file(&attachment_url, media_dir).await
                .map_err(|_| ValidationError("failed to fetch attachment"))?;
            log::info!("downloaded attachment {}", attachment_url);
            downloaded.push((
                file_name,
                attachment.media_type.or(media_type),
            ));
        };
        for (file_name, media_type) in downloaded {
            let db_attachment = create_attachment(
                db_client,
                &author.id,
                file_name,
                media_type,
            ).await?;
            attachments.push(db_attachment.id);
        };
    };
    let mut mentions: Vec<Uuid> = Vec::new();
    let mut tags = vec![];
    if let Some(list) = object.tag {
        for tag in list {
            if tag.tag_type == HASHTAG {
                if let Some(tag_name) = tag.name {
                    // Ignore invalid tags
                    if let Ok(tag_name) = normalize_tag(&tag_name) {
                        tags.push(tag_name);
                    };
                };
            } else if tag.tag_type == MENTION {
                // Try to find profile by actor ID.
                if let Some(href) = tag.href {
                    if let Ok(username) = parse_actor_id(&instance.url(), &href) {
                        let user = get_user_by_name(db_client, &username).await?;
                        if !mentions.contains(&user.id) {
                            mentions.push(user.id);
                        };
                        continue;
                    };
                    // WARNING: `href` attribute is usually actor ID
                    // but also can be actor URL (profile link).
                    // This may lead to failed import due to
                    // unique constraint violation on DB insert.
                    match get_or_import_profile_by_actor_id(
                        db_client,
                        instance,
                        media_dir,
                        &href,
                    ).await {
                        Ok(profile) => {
                            if !mentions.contains(&profile.id) {
                                mentions.push(profile.id);
                            };
                            continue;
                        },
                        Err(error) => {
                            log::warn!("failed to find mentioned profile {}: {}", href, error);
                        },
                    };
                };
                // Try to find profile by actor address
                let tag_name = match tag.name {
                    Some(name) => name,
                    None => {
                        log::warn!("failed to parse mention");
                        continue;
                    },
                };
                if let Ok(actor_address) = mention_to_address(
                    &instance.host(),
                    &tag_name,
                ) {
                    let profile = match get_profile_by_acct(
                        db_client,
                        &actor_address.acct(),
                    ).await {
                        Ok(profile) => profile,
                        Err(DatabaseError::NotFound(_)) => {
                            match import_profile_by_actor_address(
                                db_client,
                                instance,
                                media_dir,
                                &actor_address,
                            ).await {
                                Ok(profile) => profile,
                                Err(ImportError::FetchError(error)) => {
                                    // Ignore mention if fetcher fails
                                    log::warn!("{}", error);
                                    continue;
                                },
                                Err(other_error) => {
                                    return Err(other_error);
                                },
                            }
                        },
                        Err(other_error) => return Err(other_error.into()),
                    };
                    if !mentions.contains(&profile.id) {
                        mentions.push(profile.id);
                    };
                } else {
                    log::warn!("failed to parse mention {}", tag_name);
                };
            };
        };
    };
    let in_reply_to_id = match object.in_reply_to {
        Some(object_id) => {
            match parse_object_id(&instance.url(), &object_id) {
                Ok(post_id) => {
                    // Local post
                    let post = get_post_by_id(db_client, &post_id).await?;
                    Some(post.id)
                },
                Err(_) => {
                    let note_id = redirects.get(&object_id)
                        .unwrap_or(&object_id);
                    let post = get_post_by_object_id(db_client, note_id).await?;
                    Some(post.id)
                },
            }
        },
        None => None,
    };
    let primary_audience = match object.to {
        Some(value) => {
            parse_array(&value)
                .map_err(|_| ValidationError("invalid 'to' property value"))?
        },
        None => vec![],
    };
    let secondary_audience = match object.cc {
        Some(value) => {
            parse_array(&value)
                .map_err(|_| ValidationError("invalid 'cc' property value"))?
        },
        None => vec![],
    };
    let visibility = get_note_visibility(
        &author,
        primary_audience,
        secondary_audience,
    );
    if visibility != Visibility::Public {
        log::warn!(
            "processing note with visibility {:?} attributed to {}",
            visibility,
            author.username,
        );
    };
    let post_data = PostCreateData {
        content: content,
        in_reply_to_id,
        repost_of_id: None,
        visibility,
        attachments: attachments,
        mentions: mentions,
        tags: tags,
        object_id: Some(object.id),
        created_at: object.published,
    };
    let post = create_post(db_client, &author.id, post_data).await?;
    Ok(post)
}

#[cfg(test)]
mod tests {
    use crate::activitypub::activity::Object;
    use crate::activitypub::actor::Actor;
    use crate::activitypub::vocabulary::NOTE;
    use super::*;

    #[test]
    fn test_get_note_content() {
        let object = Object {
            content: Some("test".to_string()),
            object_type: NOTE.to_string(),
            ..Default::default()
        };
        let content = get_note_content(&object).unwrap();
        assert_eq!(content, "test");
    }

    #[test]
    fn test_get_note_visibility_public() {
        let author = DbActorProfile::default();
        let primary_audience = vec![AP_PUBLIC.to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Public);
    }

    #[test]
    fn test_get_note_visibility_followers() {
        let author_followers = "https://example.com/users/author/followers";
        let author = DbActorProfile {
            actor_json: Some(Actor {
                followers: Some(author_followers.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let primary_audience = vec![author_followers.to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Followers);
    }

    #[test]
    fn test_get_note_visibility_direct() {
        let author = DbActorProfile::default();
        let primary_audience = vec!["https://example.com/users/1".to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Direct);
    }
}