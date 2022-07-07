use tokio_postgres::GenericClient;

use crate::errors::DatabaseError;
use crate::models::posts::queries::get_local_post_count;
use crate::models::users::queries::get_user_count;
use super::types::{Usage, Users};

pub async fn get_usage(db_client: &impl GenericClient) -> Result<Usage, DatabaseError> {
    let user_count = get_user_count(db_client).await?;
    let post_count = get_local_post_count(db_client).await?;
    let usage = Usage {
        users: Users { total: user_count },
        local_posts: post_count,
    };
    Ok(usage)
}
