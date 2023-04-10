use clap::Parser;

use mitra::logger::configure_logger;
use mitra_config::parse_config;
use mitra_models::database::create_database_client;
use mitra_models::database::migrate::apply_migrations;

mod cli;
use cli::{Opts, SubCommand};

#[tokio::main]
async fn main() {
    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommand::GenerateRsaKey(cmd) => cmd.execute(),
        SubCommand::GenerateEthereumAddress(cmd) => cmd.execute(),
        subcmd => {
            // Other commands require initialized app
            let (config, config_warnings) = parse_config();
            configure_logger(config.log_level);
            log::info!("config loaded from {}", config.config_path);
            for warning in config_warnings {
                log::warn!("{}", warning);
            };

            let db_config = config.database_url.parse().unwrap();
            let db_client = &mut create_database_client(&db_config).await;
            apply_migrations(db_client).await;

            match subcmd {
                SubCommand::GenerateInviteCode(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::ListInviteCodes(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::CreateUser(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::SetPassword(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::SetRole(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::RefetchActor(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ReadOutbox(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteProfile(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeletePost(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteExtraneousPosts(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteUnusedAttachments(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteOrphanedFiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteEmptyProfiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::PruneRemoteEmojis(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ListUnreachableActors(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ImportEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::UpdateCurrentBlock(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ResetSubscriptions(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::CreateMoneroWallet(cmd) => cmd.execute(&config).await.unwrap(),
                SubCommand::CheckExpiredInvoice(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                _ => unreachable!(),
            };
        },
    };
}
