use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::str::FromStr;

use rsa::RsaPrivateKey;

use mitra_utils::{
    crypto_rsa::{deserialize_private_key, generate_rsa_key, serialize_private_key},
    files::{set_file_permissions, write_file},
};

use super::config::Config;
use super::environment::Environment;
use super::registration::{DefaultRole, RegistrationType};

struct EnvConfig {
    config_path: String,
    environment: Option<Environment>,
}

#[cfg(feature = "production")]
const DEFAULT_CONFIG_PATH: &str = "/etc/mitra/config.yaml";
#[cfg(not(feature = "production"))]
const DEFAULT_CONFIG_PATH: &str = "config.yaml";

fn parse_env() -> EnvConfig {
    dotenv::from_filename(".env.local").ok();
    dotenv::dotenv().ok();
    let config_path = std::env::var("CONFIG_PATH").unwrap_or(DEFAULT_CONFIG_PATH.to_string());
    let environment = std::env::var("ENVIRONMENT")
        .ok()
        .map(|val| Environment::from_str(&val).expect("invalid environment type"));
    EnvConfig {
        config_path,
        environment,
    }
}

extern "C" {
    fn geteuid() -> u32;
}

fn check_directory_owner(path: &Path) -> () {
    let metadata = std::fs::metadata(path).expect("can't read file metadata");
    let owner_uid = metadata.uid();
    let current_uid = unsafe { geteuid() };
    if owner_uid != current_uid {
        panic!(
            "{} owner ({}) is different from the current user ({})",
            path.display(),
            owner_uid,
            current_uid,
        );
    };
}

/// Generates new instance RSA key or returns existing key
fn read_instance_rsa_key(storage_dir: &Path) -> RsaPrivateKey {
    let private_key_path = storage_dir.join("instance_rsa_key");
    if private_key_path.exists() {
        let private_key_str =
            std::fs::read_to_string(&private_key_path).expect("failed to read instance RSA key");
        let private_key =
            deserialize_private_key(&private_key_str).expect("failed to read instance RSA key");
        private_key
    } else {
        let private_key = generate_rsa_key().expect("failed to generate RSA key");
        let private_key_str =
            serialize_private_key(&private_key).expect("failed to serialize RSA key");
        write_file(private_key_str.as_bytes(), &private_key_path)
            .expect("failed to write instance RSA key");
        set_file_permissions(&private_key_path, 0o600)
            .expect("failed to set permissions on RSA key file");
        private_key
    }
}

pub fn parse_config() -> (Config, Vec<&'static str>) {
    let env = parse_env();
    let config_yaml =
        std::fs::read_to_string(&env.config_path).expect("failed to load config file");
    let mut config = serde_yaml::from_str::<Config>(&config_yaml).expect("invalid yaml data");
    let mut warnings = vec![];

    // Set parameters from environment
    config.config_path = env.config_path;
    if let Some(environment) = env.environment {
        // Overwrite default only if ENVIRONMENT variable is set
        config.environment = environment;
    };

    // Validate config
    if !config.storage_dir.exists() {
        panic!("storage directory does not exist");
    };
    check_directory_owner(&config.storage_dir);
    config.try_instance_url().expect("invalid instance URI");
    if config.ipfs_api_url.is_some() != config.ipfs_gateway_url.is_some() {
        panic!("both ipfs_api_url and ipfs_gateway_url must be set");
    };

    // Migrations
    if let Some(registrations_open) = config.registrations_open {
        // Change type if 'registrations_open' parameter is used
        warnings
            .push("'registrations_open' setting is deprecated, use 'registration.type' instead");
        if registrations_open {
            config.registration.registration_type = RegistrationType::Open;
        } else {
            config.registration.registration_type = RegistrationType::Invite;
        };
    };
    if let Some(read_only_user) = config.registration.default_role_read_only_user {
        warnings.push("'default_role_read_only_user' setting is deprecated, use 'registration.default_role' instead");
        if read_only_user {
            config.registration.default_role = DefaultRole::ReadOnlyUser;
        } else {
            config.registration.default_role = DefaultRole::NormalUser;
        };
    };
    if let Some(post_character_limit) = config.post_character_limit {
        warnings.push("'post_character_limit' setting is deprecated, use 'limits.posts.character_limit' instead");
        config.limits.posts.character_limit = post_character_limit;
    };
    if let Some(ref proxy_url) = config.proxy_url {
        warnings.push("'proxy_url' setting is deprecated, use 'federation.proxy_url' instead");
        config.federation.proxy_url = Some(proxy_url.to_string());
    };

    // Insert instance RSA key
    config.instance_rsa_key = Some(read_instance_rsa_key(&config.storage_dir));

    (config, warnings)
}
