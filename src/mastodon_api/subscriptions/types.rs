use serde::Deserialize;

#[derive(Deserialize)]
pub struct SubscriptionQueryParams {
    pub price: u64,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum SubscriptionSettings {
    #[serde(rename = "ethereum")]
    Ethereum,
    #[serde(rename = "monero")]
    Monero { price: u64, payout_address: String },
}
