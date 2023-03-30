use crate::database::{DatabaseClient, DatabaseError};

use super::types::DbEmoji;
use super::queries::{
    get_local_emoji_by_name,
    get_emoji_by_name_and_hostname,
};

pub async fn get_emoji_by_name(
    db_client: &impl DatabaseClient,
    emoji_name: &str,
    maybe_hostname: Option<&str>,
) -> Result<DbEmoji, DatabaseError> {
    if let Some(hostname) = maybe_hostname {
        get_emoji_by_name_and_hostname(db_client, emoji_name, hostname).await
    } else {
        get_local_emoji_by_name(db_client, emoji_name).await
    }
}
