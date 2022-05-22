use chrono::{Duration, Utc};
use clap::Parser;
use uuid::Uuid;

use mitra::activitypub::fetcher::fetchers::fetch_actor;
use mitra::activitypub::inbox::update_person::update_actor;
use mitra::config;
use mitra::database::create_database_client;
use mitra::database::migrate::apply_migrations;
use mitra::ethereum::signatures::generate_ecdsa_key;
use mitra::ethereum::utils::key_to_ethereum_address;
use mitra::logger::configure_logger;
use mitra::models::attachments::queries::delete_unused_attachments;
use mitra::models::posts::queries::{delete_post, find_extraneous_posts};
use mitra::models::profiles::queries::delete_profile;
use mitra::models::users::queries::{
    create_invite_code,
    get_invite_codes,
};
use mitra::utils::crypto::{generate_private_key, serialize_private_key};

/// Admin CLI tool
#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    GenerateRsaKey(GenerateRsaKey),
    GenerateEthereumAddress(GenerateEthereumAddress),

    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    RefetchActor(RefetchActor),
    DeleteProfile(DeleteProfile),
    DeletePost(DeletePost),
    DeleteExtraneousPosts(DeleteExtraneousPosts),
    DeleteUnusedAttachments(DeleteUnusedAttachments),
}

/// Generate RSA private key
#[derive(Parser)]
struct GenerateRsaKey;

impl GenerateRsaKey {
    fn execute(&self) -> () {
        let private_key = generate_private_key().unwrap();
        let private_key_str = serialize_private_key(&private_key).unwrap();
        println!("{}", private_key_str);
    }
}

/// Generate ethereum address
#[derive(Parser)]
struct GenerateEthereumAddress;

/// Generate invite code
#[derive(Parser)]
struct GenerateInviteCode;

/// List invite codes
#[derive(Parser)]
struct ListInviteCodes;

/// Re-fetch actor profile by actor ID
#[derive(Parser)]
struct RefetchActor {
    #[clap(short)]
    id: String,
}

/// Delete profile
#[derive(Parser)]
struct DeleteProfile {
    #[clap(short)]
    id: Uuid,
}

/// Delete post
#[derive(Parser)]
struct DeletePost {
    #[clap(short)]
    id: Uuid,
}

/// Delete old remote posts
#[derive(Parser)]
struct DeleteExtraneousPosts {
    #[clap(short)]
    days: i64,

    #[clap(long)]
    dry_run: bool,
}

/// Delete attachments that doesn't belong to any post
#[derive(Parser)]
struct DeleteUnusedAttachments {
    #[clap(short)]
    days: i64,
}

#[tokio::main]
async fn main() {
    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommand::GenerateRsaKey(cmd) => cmd.execute(),
        SubCommand::GenerateEthereumAddress(_) => {
            let private_key = generate_ecdsa_key();
            let address = key_to_ethereum_address(&private_key);
            println!(
                "address {:?}; private key {}",
                address, private_key.display_secret(),
            );
        },
        subcmd => {
            // Other commands require initialized app
            let config = config::parse_config();
            configure_logger(config.log_level);
            log::info!("config loaded from {}", config.config_path);
            let db_config = config.database_url.parse().unwrap();
            let db_client = &mut create_database_client(&db_config).await;
            apply_migrations(db_client).await;

            match subcmd {
                SubCommand::GenerateInviteCode(_) => {
                    let invite_code = create_invite_code(db_client).await.unwrap();
                    println!("generated invite code: {}", invite_code);
                },
                SubCommand::ListInviteCodes(_) => {
                    let invite_codes = get_invite_codes(db_client).await.unwrap();
                    if invite_codes.is_empty() {
                        println!("no invite codes found");
                        return;
                    };
                    for code in invite_codes {
                        println!("{}", code);
                    };
                },
                SubCommand::RefetchActor(subopts) => {
                    let actor_id = subopts.id;
                    let actor = fetch_actor(&config.instance(), &actor_id).await.unwrap();
                    update_actor(db_client, &config.media_dir(), actor).await.unwrap();
                    println!("profile updated");
                },
                SubCommand::DeleteProfile(subopts) => {
                    let deletion_queue = delete_profile(db_client, &subopts.id).await.unwrap();
                    deletion_queue.process(&config).await;
                    println!("profile deleted");
                },
                SubCommand::DeletePost(subopts) => {
                    let deletion_queue = delete_post(db_client, &subopts.id).await.unwrap();
                    deletion_queue.process(&config).await;
                    println!("post deleted");
                },
                SubCommand::DeleteExtraneousPosts(subopts) => {
                    let created_before = Utc::now() - Duration::days(subopts.days);
                    let posts = find_extraneous_posts(db_client, &created_before).await.unwrap();
                    for post_id in posts {
                        if !subopts.dry_run {
                            let deletion_queue = delete_post(db_client, &post_id).await.unwrap();
                            deletion_queue.process(&config).await;
                        };
                        println!("post {} deleted", post_id);
                    };
                },
                SubCommand::DeleteUnusedAttachments(subopts) => {
                    let created_before = Utc::now() - Duration::days(subopts.days);
                    let deletion_queue = delete_unused_attachments(
                        db_client,
                        &created_before,
                    ).await.unwrap();
                    deletion_queue.process(&config).await;
                    println!("unused attachments deleted");
                },
                _ => panic!(),
            };
        },
    };
}
