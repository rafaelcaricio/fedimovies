pub mod accounts;
pub mod directory;
pub mod instance;
pub mod markers;
pub mod media;
pub mod notifications;
pub mod oauth;
mod pagination;
pub mod search;
pub mod statuses;
pub mod timelines;
mod uploads;

const MASTODON_API_VERSION: &str = "3.0.0";
