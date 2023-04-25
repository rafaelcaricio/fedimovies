use serde::Serialize;

use fedimovies_config::{Config, RegistrationType, REEF_VERSION};
use fedimovies_utils::markdown::markdown_to_html;

use crate::mastodon_api::MASTODON_API_VERSION;
use crate::media::SUPPORTED_MEDIA_TYPES;
use crate::validators::posts::ATTACHMENT_LIMIT;

#[derive(Serialize)]
struct InstanceStats {
    user_count: i64,
    status_count: i64,
    domain_count: i64,
}

#[derive(Serialize)]
struct InstanceStatusLimits {
    max_characters: usize,
    max_media_attachments: usize,
}

#[derive(Serialize)]
struct InstanceMediaLimits {
    supported_mime_types: Vec<String>,
    image_size_limit: usize,
}

#[derive(Serialize)]
struct InstanceConfiguration {
    statuses: InstanceStatusLimits,
    media_attachments: InstanceMediaLimits,
}

/// https://docs.joinmastodon.org/entities/V1_Instance/
#[derive(Serialize)]
pub struct InstanceInfo {
    uri: String,
    title: String,
    short_description: String,
    description: String,
    description_source: String,
    version: String,
    registrations: bool,
    approval_required: bool,
    invites_enabled: bool,
    stats: InstanceStats,
    configuration: InstanceConfiguration,

    login_message: String,
    post_character_limit: usize, // deprecated
    ipfs_gateway_url: Option<String>,
}

fn get_full_api_version(version: &str) -> String {
    format!("{0} (compatible; Reef {1})", MASTODON_API_VERSION, version,)
}

impl InstanceInfo {
    pub fn create(config: &Config, user_count: i64, post_count: i64, peer_count: i64) -> Self {
        Self {
            uri: config.instance().hostname(),
            title: config.instance_title.clone(),
            short_description: config.instance_short_description.clone(),
            description: markdown_to_html(&config.instance_description),
            description_source: config.instance_description.clone(),
            version: get_full_api_version(REEF_VERSION),
            registrations: config.registration.registration_type != RegistrationType::Invite,
            approval_required: false,
            invites_enabled: config.registration.registration_type == RegistrationType::Invite,
            stats: InstanceStats {
                user_count,
                status_count: post_count,
                domain_count: peer_count,
            },
            configuration: InstanceConfiguration {
                statuses: InstanceStatusLimits {
                    max_characters: config.limits.posts.character_limit,
                    max_media_attachments: ATTACHMENT_LIMIT,
                },
                media_attachments: InstanceMediaLimits {
                    supported_mime_types: SUPPORTED_MEDIA_TYPES
                        .iter()
                        .map(|media_type| media_type.to_string())
                        .collect(),
                    image_size_limit: config.limits.media.file_size_limit,
                },
            },
            login_message: config.login_message.clone(),
            post_character_limit: config.limits.posts.character_limit,
            ipfs_gateway_url: config.ipfs_gateway_url.clone(),
        }
    }
}
