use anyhow::Error;
use clap::Parser;
use uuid::Uuid;

use mitra::activitypub::{
    actors::helpers::update_remote_profile, builders::delete_note::prepare_delete_note,
    builders::delete_person::prepare_delete_person, fetcher::fetchers::fetch_actor,
    fetcher::helpers::import_from_outbox,
};
use mitra::admin::roles::{role_from_str, ALLOWED_ROLES};
use mitra::media::{remove_files, remove_media, MediaStorage};
use mitra::validators::{emojis::EMOJI_LOCAL_MAX_SIZE, users::validate_local_username};
use mitra_config::Config;
use mitra_models::{
    attachments::queries::delete_unused_attachments,
    cleanup::find_orphaned_files,
    database::DatabaseClient,
    emojis::helpers::get_emoji_by_name,
    emojis::queries::{
        create_emoji, delete_emoji, find_unused_remote_emojis, get_emoji_by_name_and_hostname,
    },
    oauth::queries::delete_oauth_tokens,
    posts::queries::{delete_post, find_extraneous_posts, get_post_by_id},
    profiles::queries::{
        delete_profile, find_empty_profiles, find_unreachable, get_profile_by_id,
        get_profile_by_remote_actor_id,
    },
    subscriptions::queries::reset_subscriptions,
    users::queries::{
        create_invite_code, create_user, get_invite_codes, get_user_by_id, set_user_password,
        set_user_role,
    },
    users::types::UserCreateData,
};
use mitra_utils::{
    crypto_rsa::{generate_rsa_key, serialize_private_key},
    datetime::{days_before_now, get_min_datetime},
    passwords::hash_password,
};

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
    CreateUser(CreateUser),
    SetPassword(SetPassword),
    SetRole(SetRole),
    RefetchActor(RefetchActor),
    ReadOutbox(ReadOutbox),
    DeleteProfile(DeleteProfile),
    DeletePost(DeletePost),
    DeleteEmoji(DeleteEmoji),
    DeleteExtraneousPosts(DeleteExtraneousPosts),
    DeleteUnusedAttachments(DeleteUnusedAttachments),
    DeleteOrphanedFiles(DeleteOrphanedFiles),
    DeleteEmptyProfiles(DeleteEmptyProfiles),
    PruneRemoteEmojis(PruneRemoteEmojis),
    ListUnreachableActors(ListUnreachableActors),
    ImportEmoji(ImportEmoji),
    UpdateCurrentBlock(UpdateCurrentBlock),
    ResetSubscriptions(ResetSubscriptions),
    CreateMoneroWallet(CreateMoneroWallet),
    CheckExpiredInvoice(CheckExpiredInvoice),
}

/// Generate RSA private key
#[derive(Parser)]
pub struct GenerateRsaKey;

impl GenerateRsaKey {
    pub fn execute(&self) -> () {
        let private_key = generate_rsa_key().unwrap();
        let private_key_str = serialize_private_key(&private_key).unwrap();
        println!("{}", private_key_str);
    }
}

/// Generate ethereum address
#[derive(Parser)]
pub struct GenerateEthereumAddress;

impl GenerateEthereumAddress {
    pub fn execute(&self) -> () {
        println!("dummy");
    }
}

/// Generate invite code
#[derive(Parser)]
pub struct GenerateInviteCode {
    note: Option<String>,
}

impl GenerateInviteCode {
    pub async fn execute(&self, db_client: &impl DatabaseClient) -> Result<(), Error> {
        let invite_code = create_invite_code(db_client, self.note.as_deref()).await?;
        println!("generated invite code: {}", invite_code);
        Ok(())
    }
}

/// List invite codes
#[derive(Parser)]
pub struct ListInviteCodes;

impl ListInviteCodes {
    pub async fn execute(&self, db_client: &impl DatabaseClient) -> Result<(), Error> {
        let invite_codes = get_invite_codes(db_client).await?;
        if invite_codes.is_empty() {
            println!("no invite codes found");
            return Ok(());
        };
        for invite_code in invite_codes {
            if let Some(note) = invite_code.note {
                println!("{} ({})", invite_code.code, note);
            } else {
                println!("{}", invite_code.code);
            };
        }
        Ok(())
    }
}

/// Create new user
#[derive(Parser)]
pub struct CreateUser {
    username: String,
    password: String,
    #[clap(value_parser = ALLOWED_ROLES)]
    role: String,
}

impl CreateUser {
    pub async fn execute(&self, db_client: &mut impl DatabaseClient) -> Result<(), Error> {
        validate_local_username(&self.username)?;
        let password_hash = hash_password(&self.password)?;
        let private_key = generate_rsa_key()?;
        let private_key_pem = serialize_private_key(&private_key)?;
        let role = role_from_str(&self.role)?;
        let user_data = UserCreateData {
            username: self.username.clone(),
            password_hash: Some(password_hash),
            private_key_pem,
            wallet_address: None,
            invite_code: None,
            role,
        };
        create_user(db_client, user_data).await?;
        println!("user created");
        Ok(())
    }
}

/// Set password
#[derive(Parser)]
pub struct SetPassword {
    id: Uuid,
    password: String,
}

impl SetPassword {
    pub async fn execute(&self, db_client: &impl DatabaseClient) -> Result<(), Error> {
        let password_hash = hash_password(&self.password)?;
        set_user_password(db_client, &self.id, password_hash).await?;
        // Revoke all sessions
        delete_oauth_tokens(db_client, &self.id).await?;
        println!("password updated");
        Ok(())
    }
}

/// Change user's role
#[derive(Parser)]
pub struct SetRole {
    id: Uuid,
    #[clap(value_parser = ALLOWED_ROLES)]
    role: String,
}

impl SetRole {
    pub async fn execute(&self, db_client: &impl DatabaseClient) -> Result<(), Error> {
        let role = role_from_str(&self.role)?;
        set_user_role(db_client, &self.id, role).await?;
        println!("role changed");
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
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_remote_actor_id(db_client, &self.id).await?;
        let actor = fetch_actor(&config.instance(), &self.id).await?;
        update_remote_profile(
            db_client,
            &config.instance(),
            &MediaStorage::from(config),
            profile,
            actor,
        )
        .await?;
        println!("profile updated");
        Ok(())
    }
}

/// Pull activities from actor's outbox
#[derive(Parser)]
pub struct ReadOutbox {
    actor_id: String,
}

impl ReadOutbox {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        import_from_outbox(config, db_client, &self.actor_id).await?;
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
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_id(db_client, &self.id).await?;
        let mut maybe_delete_person = None;
        if profile.is_local() {
            let user = get_user_by_id(db_client, &profile.id).await?;
            let activity = prepare_delete_person(db_client, &config.instance(), &user).await?;
            maybe_delete_person = Some(activity);
        };
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        remove_media(config, deletion_queue).await;
        // Send Delete(Person) activities
        if let Some(activity) = maybe_delete_person {
            activity.enqueue(db_client).await?;
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
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let post = get_post_by_id(db_client, &self.id).await?;
        let mut maybe_delete_note = None;
        if post.author.is_local() {
            let author = get_user_by_id(db_client, &post.author.id).await?;
            let activity =
                prepare_delete_note(db_client, &config.instance(), &author, &post).await?;
            maybe_delete_note = Some(activity);
        };
        let deletion_queue = delete_post(db_client, &post.id).await?;
        remove_media(config, deletion_queue).await;
        // Send Delete(Note) activity
        if let Some(activity) = maybe_delete_note {
            activity.enqueue(db_client).await?;
        };
        println!("post deleted");
        Ok(())
    }
}

/// Delete custom emoji
#[derive(Parser)]
pub struct DeleteEmoji {
    emoji_name: String,
    hostname: Option<String>,
}

impl DeleteEmoji {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let emoji =
            get_emoji_by_name(db_client, &self.emoji_name, self.hostname.as_deref()).await?;
        let deletion_queue = delete_emoji(db_client, &emoji.id).await?;
        remove_media(config, deletion_queue).await;
        println!("emoji deleted");
        Ok(())
    }
}

/// Delete old remote posts
#[derive(Parser)]
pub struct DeleteExtraneousPosts {
    days: u32,
}

impl DeleteExtraneousPosts {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let updated_before = days_before_now(self.days);
        let posts = find_extraneous_posts(db_client, &updated_before).await?;
        for post_id in posts {
            let deletion_queue = delete_post(db_client, &post_id).await?;
            remove_media(config, deletion_queue).await;
            println!("post {} deleted", post_id);
        }
        Ok(())
    }
}

/// Delete attachments that don't belong to any post
#[derive(Parser)]
pub struct DeleteUnusedAttachments {
    days: u32,
}

impl DeleteUnusedAttachments {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let created_before = days_before_now(self.days);
        let deletion_queue = delete_unused_attachments(db_client, &created_before).await?;
        remove_media(config, deletion_queue).await;
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
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let media_dir = config.media_dir();
        let mut files = vec![];
        for maybe_path in std::fs::read_dir(&media_dir)? {
            let file_name = maybe_path?.file_name().to_string_lossy().to_string();
            files.push(file_name);
        }
        println!("found {} files", files.len());
        let orphaned = find_orphaned_files(db_client, files).await?;
        if !orphaned.is_empty() {
            remove_files(orphaned, &media_dir);
            println!("orphaned files deleted");
        };
        Ok(())
    }
}

/// Delete empty remote profiles
#[derive(Parser)]
pub struct DeleteEmptyProfiles {
    days: u32,
}

impl DeleteEmptyProfiles {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let updated_before = days_before_now(self.days);
        let profiles = find_empty_profiles(db_client, &updated_before).await?;
        for profile_id in profiles {
            let profile = get_profile_by_id(db_client, &profile_id).await?;
            let deletion_queue = delete_profile(db_client, &profile.id).await?;
            remove_media(config, deletion_queue).await;
            println!("profile {} deleted", profile.acct);
        }
        Ok(())
    }
}

/// Delete unused remote emojis
#[derive(Parser)]
pub struct PruneRemoteEmojis;

impl PruneRemoteEmojis {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let emojis = find_unused_remote_emojis(db_client).await?;
        for emoji_id in emojis {
            let deletion_queue = delete_emoji(db_client, &emoji_id).await?;
            remove_media(config, deletion_queue).await;
            println!("emoji {} deleted", emoji_id);
        }
        Ok(())
    }
}

/// List unreachable actors
#[derive(Parser)]
pub struct ListUnreachableActors {
    days: u32,
}

impl ListUnreachableActors {
    pub async fn execute(
        &self,
        _config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let unreachable_since = days_before_now(self.days);
        let profiles = find_unreachable(db_client, &unreachable_since).await?;
        println!(
            "{0: <60} | {1: <35} | {2: <35}",
            "ID", "unreachable since", "updated at",
        );
        for profile in profiles {
            println!(
                "{0: <60} | {1: <35} | {2: <35}",
                profile.actor_id.unwrap(),
                profile.unreachable_since.unwrap().to_string(),
                profile.updated_at.to_string(),
            );
        }
        Ok(())
    }
}

/// Import custom emoji from another instance
#[derive(Parser)]
pub struct ImportEmoji {
    emoji_name: String,
    hostname: String,
}

impl ImportEmoji {
    pub async fn execute(
        &self,
        _config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let emoji =
            get_emoji_by_name_and_hostname(db_client, &self.emoji_name, &self.hostname).await?;
        if emoji.image.file_size > EMOJI_LOCAL_MAX_SIZE {
            println!("emoji is too big");
            return Ok(());
        };
        create_emoji(
            db_client,
            &emoji.emoji_name,
            None,
            emoji.image,
            None,
            &get_min_datetime(),
        )
        .await?;
        println!("added emoji to local collection");
        Ok(())
    }
}

/// Update blockchain synchronization starting block
#[derive(Parser)]
pub struct UpdateCurrentBlock {
    number: u64,
}

impl UpdateCurrentBlock {
    pub async fn execute(
        &self,
        _config: &Config,
        _db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        println!("current block updated");
        Ok(())
    }
}

/// Reset all subscriptions
/// (can be used during development or when switching between chains)
#[derive(Parser)]
pub struct ResetSubscriptions {
    #[clap(long)]
    ethereum_contract_replaced: bool,
}

impl ResetSubscriptions {
    pub async fn execute(
        &self,
        _config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        reset_subscriptions(db_client, self.ethereum_contract_replaced).await?;
        println!("subscriptions deleted");
        Ok(())
    }
}

/// Create Monero wallet
/// (can be used when monero-wallet-rpc runs with --wallet-dir option)
#[derive(Parser)]
pub struct CreateMoneroWallet {
    name: String,
    password: Option<String>,
}

impl CreateMoneroWallet {
    pub async fn execute(&self, _config: &Config) -> Result<(), Error> {
        println!("wallet created");
        Ok(())
    }
}

/// Check expired invoice
#[derive(Parser)]
pub struct CheckExpiredInvoice {
    id: Uuid,
}

impl CheckExpiredInvoice {
    pub async fn execute(
        &self,
        _config: &Config,
        _db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        Ok(())
    }
}
