use serde::Deserialize;

#[derive(Deserialize)]
pub struct SubscriptionQueryParams {
    pub price: u64,
}
