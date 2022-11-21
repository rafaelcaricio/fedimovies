use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use serde_json::{Value as JsonValue};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{Attachment, Link, Object, Tag},
    constants::{AP_MEDIA_TYPE, AP_PUBLIC, AS_MEDIA_TYPE},
    fetcher::fetchers::fetch_file,
    fetcher::helpers::{
        get_or_import_profile_by_actor_id,
        import_profile_by_actor_address,
    },
    identifiers::parse_local_actor_id,
    receiver::{parse_array, parse_property_value, HandlerError},
    vocabulary::*,
};
use crate::config::Instance;
use crate::errors::{ConversionError, DatabaseError, ValidationError};
use crate::models::attachments::queries::create_attachment;
use crate::models::posts::{
    hashtags::normalize_hashtag,
    helpers::get_post_by_object_id,
    mentions::mention_to_address,
    queries::create_post,
    types::{Post, PostCreateData, Visibility},
    validators::CONTENT_MAX_SIZE,
};
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

const ATTACHMENTS_MAX_NUM: usize = 15;

fn parse_object_url(value: &JsonValue) -> Result<String, ConversionError> {
    let object_url = match value {
        JsonValue::String(string) => string.to_owned(),
        other_value => {
            let links: Vec<Link> = parse_property_value(other_value)?;
            if let Some(link) = links.get(0) {
                link.href.clone()
            } else {
                return Err(ConversionError);
            }
        },
    };
    Ok(object_url)
}

pub fn get_note_content(object: &Object) -> Result<String, ValidationError> {
    let mut content = if let Some(ref content) = object.content {
        if object.media_type == Some("text/markdown".to_string()) {
            format!("<p>{}</p>", content)
        } else {
            // HTML
            content.to_string()
        }
    } else {
        // Lemmy pages and PeerTube videos have "name" property
        object.name.as_deref().unwrap_or("").to_string()
    };
    if object.object_type != NOTE {
        if let Some(ref value) = object.url {
            // Append link to object
            let object_url = parse_object_url(value)
                .map_err(|_| ValidationError("invalid object URL"))?;
            content += &format!(
                r#"<p><a href="{0}">{0}</a></p>"#,
                object_url,
            );
        };
    };
    if content.len() > CONTENT_MAX_SIZE {
        return Err(ValidationError("content is too long"));
    };
    let content_safe = clean_html(&content);
    Ok(content_safe)
}

fn get_note_visibility(
    author: &DbActorProfile,
    primary_audience: Vec<String>,
    secondary_audience: Vec<String>,
) -> Visibility {
    if primary_audience.contains(&AP_PUBLIC.to_string()) ||
            secondary_audience.contains(&AP_PUBLIC.to_string()) {
       return Visibility::Public;
    };
    let maybe_followers = author.actor_json.as_ref()
        .and_then(|actor| actor.followers.as_ref());
    if let Some(followers) = maybe_followers {
        if primary_audience.contains(followers) ||
                secondary_audience.contains(followers) {
            return Visibility::Followers;
        };
    };
    let maybe_subscribers = author.actor_json.as_ref()
        .and_then(|actor| actor.subscribers.as_ref());
    if let Some(subscribers) = maybe_subscribers {
        if primary_audience.contains(subscribers) ||
                secondary_audience.contains(subscribers) {
            return Visibility::Subscribers;
        };
    };
    Visibility::Direct
}

pub async fn handle_note(
    db_client: &mut impl GenericClient,
    instance: &Instance,
    media_dir: &Path,
    object: Object,
    redirects: &HashMap<String, String>,
) -> Result<Post, HandlerError> {
    match object.object_type.as_str() {
        NOTE => (),
        ARTICLE | QUESTION | PAGE | VIDEO => {
            log::info!("processing object of type {}", object.object_type);
        },
        other_type => {
            log::warn!("discarding object of type {}", other_type);
            return Err(ValidationError("unsupported type").into());
        },
    };

    let author_id = get_note_author_id(&object)?;
    let author = get_or_import_profile_by_actor_id(
        db_client,
        instance,
        media_dir,
        &author_id,
    ).await.map_err(|err| {
        log::warn!("failed to import {} ({})", author_id, err);
        err
    })?;
    let content = get_note_content(&object)?;
    let created_at = object.published.unwrap_or(Utc::now());

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
            let (file_name, media_type) = fetch_file(
                instance,
                &attachment_url,
                media_dir,
            ).await
                .map_err(|err| {
                    log::warn!("{}", err);
                    ValidationError("failed to fetch attachment")
                })?;
            log::info!("downloaded attachment {}", attachment_url);
            downloaded.push((
                file_name,
                attachment.media_type.or(media_type),
            ));
            // Stop downloading if limit is reached
            if downloaded.len() >= ATTACHMENTS_MAX_NUM {
                log::warn!("too many attachments");
                break;
            };
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
    if content.is_empty() && attachments.is_empty() {
        return Err(ValidationError("post is empty").into());
    };

    let mut mentions: Vec<Uuid> = Vec::new();
    let mut tags = vec![];
    let mut links = vec![];
    if let Some(value) = object.tag {
        let list: Vec<Tag> = parse_property_value(&value)
            .map_err(|_| ValidationError("invalid tag property"))?;
        for tag in list {
            if tag.tag_type == HASHTAG {
                if let Some(tag_name) = tag.name {
                    // Ignore invalid tags
                    if let Ok(tag_name) = normalize_hashtag(&tag_name) {
                        if !tags.contains(&tag_name) {
                            tags.push(tag_name);
                        };
                    };
                };
            } else if tag.tag_type == MENTION {
                // Try to find profile by actor ID.
                if let Some(href) = tag.href {
                    if let Ok(username) = parse_local_actor_id(&instance.url(), &href) {
                        let user = get_user_by_name(db_client, &username).await?;
                        if !mentions.contains(&user.id) {
                            mentions.push(user.id);
                        };
                        continue;
                    };
                    // NOTE: `href` attribute is usually actor ID
                    // but also can be actor URL (profile link).
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
                            log::warn!(
                                "failed to find mentioned profile by ID {}: {}",
                                href,
                                error,
                            );
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
                if let Ok(actor_address) = mention_to_address(&tag_name) {
                    let acct = actor_address.acct(&instance.host());
                    let profile = match get_profile_by_acct(
                        db_client,
                        &acct,
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
                                Err(HandlerError::FetchError(error)) => {
                                    // Ignore mention if fetcher fails
                                    log::warn!(
                                        "failed to find mentioned profile {}: {}",
                                        acct,
                                        error,
                                    );
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
            } else if tag.tag_type == LINK {
                if tag.media_type != Some(AP_MEDIA_TYPE.to_string()) &&
                    tag.media_type != Some(AS_MEDIA_TYPE.to_string())
                {
                    // Unknown media type
                    continue;
                };
                if let Some(ref href) = tag.href {
                    let href = redirects.get(href).unwrap_or(href);
                    let linked = get_post_by_object_id(
                        db_client,
                        &instance.url(),
                        href,
                    ).await?;
                    if !links.contains(&linked.id) {
                        links.push(linked.id);
                    };
                };
            };
        };
    };
    if let Some(ref object_id) = object.quote_url {
        let object_id = redirects.get(object_id).unwrap_or(object_id);
        let linked = get_post_by_object_id(
            db_client,
            &instance.url(),
            object_id,
        ).await?;
        if !links.contains(&linked.id) {
            links.push(linked.id);
        };
    };

    let in_reply_to_id = match object.in_reply_to {
        Some(ref object_id) => {
            let object_id = redirects.get(object_id).unwrap_or(object_id);
            let in_reply_to = get_post_by_object_id(
                db_client,
                &instance.url(),
                object_id,
            ).await?;
            Some(in_reply_to.id)
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
        links: links,
        object_id: Some(object.id),
        created_at,
    };
    let post = create_post(db_client, &author.id, post_data).await?;
    Ok(post)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::activitypub::{
        activity::Object,
        actors::types::Actor,
        vocabulary::NOTE,
    };
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
    fn test_get_note_content_from_video() {
        let object = Object {
            name: Some("test-name".to_string()),
            content: Some("test-content".to_string()),
            object_type: "Video".to_string(),
            url: Some(json!([{
                "type": "Link",
                "mediaType": "text/html",
                "href": "https://example.org/xyz",
            }])),
            ..Default::default()
        };
        let content = get_note_content(&object).unwrap();
        assert_eq!(
            content,
            r#"test-content<p><a href="https://example.org/xyz" rel="noopener">https://example.org/xyz</a></p>"#,
        );
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
    fn test_get_note_visibility_subscribers() {
        let author_followers = "https://example.com/users/author/followers";
        let author_subscribers = "https://example.com/users/author/subscribers";
        let author = DbActorProfile {
            actor_json: Some(Actor {
                followers: Some(author_followers.to_string()),
                subscribers: Some(author_subscribers.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let primary_audience = vec![author_subscribers.to_string()];
        let secondary_audience = vec![];
        let visibility = get_note_visibility(
            &author,
            primary_audience,
            secondary_audience,
        );
        assert_eq!(visibility, Visibility::Subscribers);
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
