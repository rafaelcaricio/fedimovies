/// http://nodeinfo.diaspora.software/schema.html

use serde::Serialize;

use crate::config::Config;

#[derive(Serialize)]
struct Software {
    name: String,
    version: String,
}

#[derive(Serialize)]
struct Services {
    inbound: Vec<String>,
    outbound: Vec<String>,
}

#[derive(Serialize)]
pub struct Users {
    pub total: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub users: Users,
    pub local_posts: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    node_name: String,
    node_description: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo20 {
    version: String,
    software: Software,
    protocols: Vec<String>,
    services: Services,
    open_registrations: bool,
    usage: Usage,
    metadata: Metadata,
}

impl NodeInfo20 {
    pub fn new(config: &Config, usage: Usage) -> Self {
        let software = Software {
            name: "mitra".to_string(),
            version: config.version.clone(),
        };
        let services = Services {
            inbound: vec![],
            outbound: vec!["atom1.0".to_string()],
        };
        let metadata = Metadata {
            node_name: config.instance_title.clone(),
            node_description: config.instance_short_description.clone(),
        };
        Self {
            version: "2.0".to_string(),
            software,
            protocols: vec!["activitypub".to_string()],
            services,
            open_registrations: config.registrations_open,
            usage,
            metadata,
        }
    }
}
