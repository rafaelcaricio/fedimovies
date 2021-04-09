use serde::Serialize;
use uuid::Uuid;

use crate::mastodon_api::accounts::types::Account;
use crate::models::users::types::User;

// TODO: use Account instead
#[derive(Serialize)]
pub struct ApiUser {
    pub id: Uuid,
    pub profile: Account,
    pub wallet_address: String,
}

impl ApiUser {
    pub fn from_user(user: User, instance_url: &str) -> Self {
        let account = Account::from_profile(user.profile, instance_url);
        Self {
            id: user.id,
            profile: account,
            wallet_address: user.wallet_address,
        }
    }
}
