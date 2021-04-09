/// https://webfinger.net/

use serde::{Serialize, Deserialize};

pub const JRD_CONTENT_TYPE: &str = "application/jrd+json";

#[derive(Deserialize)]
pub struct WebfingerQueryParams {
    pub resource: String,
}

#[derive(Serialize, Deserialize)]
pub struct Link {
    pub rel: String,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,

    pub href: Option<String>,
}

// https://datatracker.ietf.org/doc/html/rfc7033#section-4.4
#[derive(Serialize, Deserialize)]
pub struct JsonResourceDescriptor {
    pub subject: String,
    pub links: Vec<Link>,
}
