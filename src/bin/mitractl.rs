use anyhow::Error;
use chrono::{Duration, Utc};
use clap::Parser;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use mitra::activitypub::builders::delete_note::prepare_delete_note;
use mitra::activitypub::builders::delete_person::prepare_delete_person;
use mitra::activitypub::fetcher::fetchers::fetch_actor;
use mitra::activitypub::handlers::update_person::update_remote_profile;
use mitra::config::{parse_config, Config};
use mitra::database::create_database_client;
use mitra::database::migrate::apply_migrations;
use mitra::ethereum::signatures::generate_ecdsa_key;
use mitra::ethereum::sync::save_current_block_number;
use mitra::ethereum::utils::key_to_ethereum_address;
use mitra::logger::configure_logger;
use mitra::models::attachments::queries::delete_unused_attachments;
use mitra::models::cleanup::find_orphaned_files;
use mitra::models::posts::queries::{delete_post, find_extraneous_posts, get_post_by_id};
use mitra::models::profiles::queries::{
    delete_profile,
    get_profile_by_actor_id,
    get_profile_by_id,
    reset_subscriptions,
};
use mitra::models::users::queries::{
    create_invite_code,
    get_invite_codes,
    get_user_by_id,
};
use mitra::utils::crypto::{generate_private_key, serialize_private_key};
use mitra::utils::files::remove_files;

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
    UpdateCurrentBlock(UpdateCurrentBlock),
    DeleteOrphanedFiles(DeleteOrphanedFiles),
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
    id: String,
}

impl RefetchActor {
    async fn execute(
        &self,
        config: &Config,
        db_client: &impl GenericClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_actor_id(db_client, &self.id).await?;
        let actor = fetch_actor(&config.instance(), &self.id).await?;
        update_remote_profile(db_client, &config.media_dir(), profile, actor).await?;
        println!("profile updated");
        Ok(())
    }
}

/// Delete profile
#[derive(Parser)]
struct DeleteProfile {
    id: Uuid,
}

/// Delete post
#[derive(Parser)]
struct DeletePost {
    id: Uuid,
}

/// Delete old remote posts
#[derive(Parser)]
struct DeleteExtraneousPosts {
    days: i64,

    #[clap(long)]
    dry_run: bool,
}

/// Delete attachments that don't belong to any post
#[derive(Parser)]
struct DeleteUnusedAttachments {
    days: i64,
}

/// Find and delete orphaned files
#[derive(Parser)]
struct DeleteOrphanedFiles;

impl DeleteOrphanedFiles {
    async fn execute(
        &self,
        config: &Config,
        db_client: &impl GenericClient,
    ) -> Result<(), Error> {
        let media_dir = config.media_dir();
        let mut files = vec![];
        for maybe_path in std::fs::read_dir(&media_dir)? {
            let file_name = maybe_path?.file_name()
                .to_string_lossy().to_string();
            files.push(file_name);
        };
        println!("found {} files", files.len());
        let orphaned = find_orphaned_files(db_client, files).await?;
        if !orphaned.is_empty() {
            remove_files(orphaned, &media_dir);
            println!("orphaned files deleted");
        };
        Ok(())
    }
}

/// Update blockchain synchronization starting block
#[derive(Parser)]
struct UpdateCurrentBlock {
    number: u64,

    #[clap(long)]
    reset_db: bool,
}

impl UpdateCurrentBlock {
    async fn execute(
        &self,
        config: &Config,
        db_client: &impl GenericClient,
    ) -> Result<(), Error> {
        save_current_block_number(&config.storage_dir, self.number)?;
        if self.reset_db {
            reset_subscriptions(db_client).await?;
        };
        println!("current block updated");
        Ok(())
    }
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
            let config = parse_config();
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
                SubCommand::RefetchActor(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteProfile(subopts) => {
                    let profile = get_profile_by_id(db_client, &subopts.id).await.unwrap();
                    let mut maybe_delete_person = None;
                    if profile.is_local() {
                        let user = get_user_by_id(db_client, &profile.id).await.unwrap();
                        let activity = prepare_delete_person(db_client, config.instance(), &user)
                            .await.unwrap();
                        maybe_delete_person = Some(activity);
                    };
                    let deletion_queue = delete_profile(db_client, &profile.id).await.unwrap();
                    deletion_queue.process(&config).await;
                    // Send Delete(Person) activities
                    if let Some(activity) = maybe_delete_person {
                        activity.deliver().await.unwrap();
                    };
                    println!("profile deleted");
                },
                SubCommand::DeletePost(subopts) => {
                    let post = get_post_by_id(db_client, &subopts.id).await.unwrap();
                    let deletion_queue = delete_post(db_client, &post.id).await.unwrap();
                    deletion_queue.process(&config).await;
                    if post.author.is_local() {
                        // Send Delete(Note) activity
                        let author = get_user_by_id(db_client, &post.author.id).await.unwrap();
                        prepare_delete_note(db_client, config.instance(), &author, &post).await.unwrap()
                            .deliver().await.unwrap();
                    };
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
                SubCommand::DeleteOrphanedFiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::UpdateCurrentBlock(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                _ => panic!(),
            };
        },
    };
}
