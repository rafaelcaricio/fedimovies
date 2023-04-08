/// http://nodeinfo.diaspora.software/schema.html

use serde::Serialize;

use mitra_config::{Config, RegistrationType, MITRA_VERSION};

const MITRA_NAME: &str = "reef";
const MITRA_REPOSITORY: &str = "https://code.caric.io/reef/reef";
const ATOM_SERVICE: &str = "atom1.0";
const ACTIVITYPUB_PROTOCOL: &str = "activitypub";

#[derive(Serialize)]
struct Software20 {
    name: String,
    version: String,
}

impl Default for Software20 {
    fn default() -> Self {
        Self {
            name: MITRA_NAME.to_string(),
            version: MITRA_VERSION.to_string(),
        }
    }
}

#[derive(Serialize)]
struct Software21 {
    name: String,
    version: String,
    repository: String,
}

impl Default for Software21 {
    fn default() -> Self {
        Self {
            name: MITRA_NAME.to_string(),
            version: MITRA_VERSION.to_string(),
            repository: MITRA_REPOSITORY.to_string(),
        }
    }
}

#[derive(Serialize)]
struct Services {
    inbound: Vec<&'static str>,
    outbound: Vec<&'static str>,
}

impl Default for Services {
    fn default() -> Self {
        Self {
            inbound: vec![],
            outbound: vec![ATOM_SERVICE],
        }
    }
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
struct FederationMetadata {
    enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    node_name: String,
    node_description: String,
    federation: FederationMetadata,
}

impl Metadata {
    fn new(config: &Config) -> Self {
        Self {
            node_name: config.instance_title.clone(),
            node_description: config.instance_short_description.clone(),
            federation: FederationMetadata {
                enabled: config.federation.enabled,
            },
        }
    }
}

fn has_open_registrations(config: &Config) -> bool {
    config.registration.registration_type != RegistrationType::Invite
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo20 {
    version: &'static str,
    software: Software20,
    protocols: Vec<&'static str>,
    services: Services,
    open_registrations: bool,
    usage: Usage,
    metadata: Metadata,
}

impl NodeInfo20 {
    pub fn new(config: &Config, usage: Usage) -> Self {
        Self {
            version: "2.0",
            software: Software20::default(),
            protocols: vec![ACTIVITYPUB_PROTOCOL],
            services: Services::default(),
            open_registrations: has_open_registrations(config),
            usage,
            metadata: Metadata::new(config),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo21 {
    version: &'static str,
    software: Software21,
    protocols: Vec<&'static str>,
    services: Services,
    open_registrations: bool,
    usage: Usage,
    metadata: Metadata,
}

impl NodeInfo21 {
    pub fn new(config: &Config, usage: Usage) -> Self {
        Self {
            version: "2.1",
            software: Software21::default(),
            protocols: vec![ACTIVITYPUB_PROTOCOL],
            services: Services::default(),
            open_registrations: has_open_registrations(config),
            usage,
            metadata: Metadata::new(config),
        }
    }
}
