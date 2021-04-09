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
struct Users {
}

#[derive(Serialize)]
struct Usage {
    users: Users,
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
    pub fn new(config: &Config) -> Self {
        let software = Software {
            name: "mitra".to_string(),
            version: config.version.clone(),
        };
        let services = Services { inbound: vec![], outbound: vec![] };
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
            usage: Usage { users: Users { } },
            metadata,
        }
    }
}
