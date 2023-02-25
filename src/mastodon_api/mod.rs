pub mod accounts;
pub mod apps;
pub mod custom_emojis;
pub mod directory;
pub mod instance;
pub mod markers;
pub mod media;
pub mod notifications;
pub mod oauth;
pub mod search;
pub mod settings;
pub mod statuses;
pub mod subscriptions;
pub mod timelines;

mod errors;
mod pagination;
mod uploads;

const MASTODON_API_VERSION: &str = "4.0.0";
pub use uploads::UPLOAD_MAX_SIZE;
