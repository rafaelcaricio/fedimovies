use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::get_local_post_count,
    users::queries::get_user_count,
};

use super::types::{Usage, Users};

pub async fn get_usage(db_client: &impl DatabaseClient) -> Result<Usage, DatabaseError> {
    let user_count = get_user_count(db_client).await?;
    let post_count = get_local_post_count(db_client).await?;
    let usage = Usage {
        users: Users { total: user_count },
        local_posts: post_count,
    };
    Ok(usage)
}
