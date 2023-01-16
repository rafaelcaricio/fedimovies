use clap::Parser;

use mitra::cli::{Opts, SubCommand};
use mitra::config::parse_config;
use mitra::database::create_database_client;
use mitra::database::migrate::apply_migrations;
use mitra::logger::configure_logger;

#[tokio::main]
async fn main() {
    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommand::GenerateRsaKey(cmd) => cmd.execute(),
        SubCommand::GenerateEthereumAddress(cmd) => cmd.execute(),
        subcmd => {
            // Other commands require initialized app
            let config = parse_config();
            configure_logger(config.log_level);
            log::info!("config loaded from {}", config.config_path);
            let db_config = config.database_url.parse().unwrap();
            let db_client = &mut create_database_client(&db_config).await;
            apply_migrations(db_client).await;

            match subcmd {
                SubCommand::GenerateInviteCode(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::ListInviteCodes(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::SetPassword(cmd) => cmd.execute(db_client).await.unwrap(),
                SubCommand::RefetchActor(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteProfile(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeletePost(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteEmoji(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteExtraneousPosts(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteUnusedAttachments(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteOrphanedFiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::DeleteEmptyProfiles(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::UpdateCurrentBlock(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::ResetSubscriptions(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                SubCommand::CreateMoneroWallet(cmd) => cmd.execute(&config).await.unwrap(),
                SubCommand::CheckExpiredInvoice(cmd) => cmd.execute(&config, db_client).await.unwrap(),
                _ => panic!(),
            };
        },
    };
}
