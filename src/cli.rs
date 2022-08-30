use anyhow::{anyhow, Error};
use chrono::{Duration, Utc};
use clap::Parser;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::builders::delete_note::prepare_delete_note;
use crate::activitypub::builders::delete_person::prepare_delete_person;
use crate::activitypub::fetcher::fetchers::fetch_actor;
use crate::activitypub::handlers::update_person::update_remote_profile;
use crate::config::Config;
use crate::ethereum::signatures::generate_ecdsa_key;
use crate::ethereum::sync::save_current_block_number;
use crate::ethereum::utils::key_to_ethereum_address;
use crate::models::attachments::queries::delete_unused_attachments;
use crate::models::cleanup::find_orphaned_files;
use crate::models::posts::queries::{delete_post, find_extraneous_posts, get_post_by_id};
use crate::models::profiles::queries::{
    delete_profile,
    get_profile_by_actor_id,
    get_profile_by_id,
    reset_subscriptions,
};
use crate::models::users::queries::{
    create_invite_code,
    get_invite_codes,
    get_user_by_id,
};
use crate::monero::wallet::create_monero_wallet;
use crate::utils::crypto::{generate_private_key, serialize_private_key};
use crate::utils::files::remove_files;

/// Admin CLI tool
#[derive(Parser)]
pub struct Opts {
    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    GenerateRsaKey(GenerateRsaKey),
    GenerateEthereumAddress(GenerateEthereumAddress),

    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    RefetchActor(RefetchActor),
    DeleteProfile(DeleteProfile),
    DeletePost(DeletePost),
    DeleteExtraneousPosts(DeleteExtraneousPosts),
    DeleteUnusedAttachments(DeleteUnusedAttachments),
    DeleteOrphanedFiles(DeleteOrphanedFiles),
    UpdateCurrentBlock(UpdateCurrentBlock),
    CreateMoneroWallet(CreateMoneroWallet),
}

/// Generate RSA private key
#[derive(Parser)]
pub struct GenerateRsaKey;

impl GenerateRsaKey {
    pub fn execute(&self) -> () {
        let private_key = generate_private_key().unwrap();
        let private_key_str = serialize_private_key(&private_key).unwrap();
        println!("{}", private_key_str);
    }
}

/// Generate ethereum address
#[derive(Parser)]
pub struct GenerateEthereumAddress;

impl GenerateEthereumAddress {
    pub fn execute(&self) -> () {
        let private_key = generate_ecdsa_key();
        let address = key_to_ethereum_address(&private_key);
        println!(
            "address {:?}; private key {}",
            address, private_key.display_secret(),
        );
    }
}

/// Generate invite code
#[derive(Parser)]
pub struct GenerateInviteCode;

impl GenerateInviteCode {
    pub async fn execute(
        &self,
        db_client: &impl GenericClient,
    ) -> Result<(), Error> {
        let invite_code = create_invite_code(db_client).await?;
        println!("generated invite code: {}", invite_code);
        Ok(())
    }
}

/// List invite codes
#[derive(Parser)]
pub struct ListInviteCodes;

impl ListInviteCodes {
    pub async fn execute(
        &self,
        db_client: &impl GenericClient,
    ) -> Result<(), Error> {
        let invite_codes = get_invite_codes(db_client).await?;
        if invite_codes.is_empty() {
            println!("no invite codes found");
            return Ok(());
        };
        for code in invite_codes {
            println!("{}", code);
        };
        Ok(())
    }
}

/// Re-fetch actor profile by actor ID
#[derive(Parser)]
pub struct RefetchActor {
    id: String,
}

impl RefetchActor {
    pub async fn execute(
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
pub struct DeleteProfile {
    id: Uuid,
}

impl DeleteProfile {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl GenericClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_id(db_client, &self.id).await?;
        let mut maybe_delete_person = None;
        if profile.is_local() {
            let user = get_user_by_id(db_client, &profile.id).await?;
            let activity =
                prepare_delete_person(db_client, config.instance(), &user).await?;
            maybe_delete_person = Some(activity);
        };
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        deletion_queue.process(config).await;
        // Send Delete(Person) activities
        if let Some(activity) = maybe_delete_person {
            activity.deliver().await?;
        };
        println!("profile deleted");
        Ok(())
    }
}

/// Delete post
#[derive(Parser)]
pub struct DeletePost {
    id: Uuid,
}

impl DeletePost {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl GenericClient,
    ) -> Result<(), Error> {
        let post = get_post_by_id(db_client, &self.id).await?;
        let deletion_queue = delete_post(db_client, &post.id).await?;
        deletion_queue.process(config).await;
        if post.author.is_local() {
            // Send Delete(Note) activity
            let author = get_user_by_id(db_client, &post.author.id).await?;
            prepare_delete_note(db_client, config.instance(), &author, &post)
                .await?
                .deliver().await?;
        };
        println!("post deleted");
        Ok(())
    }
}

/// Delete old remote posts
#[derive(Parser)]
pub struct DeleteExtraneousPosts {
    days: i64,
}

impl DeleteExtraneousPosts {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl GenericClient,
    ) -> Result<(), Error> {
        let created_before = Utc::now() - Duration::days(self.days);
        let posts = find_extraneous_posts(db_client, &created_before).await?;
        for post_id in posts {
            let deletion_queue = delete_post(db_client, &post_id).await?;
            deletion_queue.process(config).await;
            println!("post {} deleted", post_id);
        };
        Ok(())
    }
}

/// Delete attachments that don't belong to any post
#[derive(Parser)]
pub struct DeleteUnusedAttachments {
    days: i64,
}

impl DeleteUnusedAttachments {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl GenericClient,
    ) -> Result<(), Error> {
        let created_before = Utc::now() - Duration::days(self.days);
        let deletion_queue = delete_unused_attachments(
            db_client,
            &created_before,
        ).await?;
        deletion_queue.process(config).await;
        println!("unused attachments deleted");
        Ok(())
    }
}

/// Find and delete orphaned files
#[derive(Parser)]
pub struct DeleteOrphanedFiles;

impl DeleteOrphanedFiles {
    pub async fn execute(
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
pub struct UpdateCurrentBlock {
    number: u64,

    #[clap(long)]
    reset_db: bool,
}

impl UpdateCurrentBlock {
    pub async fn execute(
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

/// Create Monero wallet
#[derive(Parser)]
pub struct CreateMoneroWallet;

impl CreateMoneroWallet {
    pub async fn execute(
        &self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.blockchain()
            .and_then(|conf| conf.monero_config())
            .ok_or(anyhow!("monero configuration not found"))?;
        create_monero_wallet(monero_config).await?;
        println!("wallet created");
        Ok(())
    }
}
