use clap::Clap;
use tokio;
use uuid::Uuid;

use mitra::config;
use mitra::database::{create_pool, get_database_client};
use mitra::database::migrate::apply_migrations;
use mitra::ethereum::utils::generate_ethereum_address;
use mitra::logger::configure_logger;
use mitra::models::posts::queries::delete_post;
use mitra::models::profiles::queries::delete_profile;
use mitra::models::users::queries::{
    create_invite_code,
    get_invite_codes,
};

/// Admin CLI tool
#[derive(Clap)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    DeleteProfile(DeleteProfile),
    DeletePost(DeletePost),
    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    GenerateEthereumAddress(GenerateEthereumAddress),
}

/// Delete profile
#[derive(Clap)]
struct DeleteProfile {
    #[clap(short)]
    id: Uuid,
}

/// Delete post
#[derive(Clap)]
struct DeletePost {
    #[clap(short)]
    id: Uuid,
}

/// Generate invite code
#[derive(Clap)]
struct GenerateInviteCode { }

/// List invite codes
#[derive(Clap)]
struct ListInviteCodes { }

/// Generate ethereum address
#[derive(Clap)]
struct GenerateEthereumAddress { }

#[tokio::main]
async fn main() {
    let config = config::parse_config();
    configure_logger();
    let db_pool = create_pool(&config.database_url);
    apply_migrations(&db_pool).await;
    let db_client = &mut **get_database_client(&db_pool).await.unwrap();
    let opts: Opts = Opts::parse();

    match opts.subcmd {
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
        SubCommand::GenerateInviteCode(_) => {
            let invite_code = create_invite_code(db_client).await.unwrap();
            println!("generated invite code: {}", invite_code);
        },
        SubCommand::ListInviteCodes(_) => {
            let invite_codes = get_invite_codes(db_client).await.unwrap();
            if invite_codes.len() == 0 {
                println!("no invite codes found");
                return;
            }
            for code in invite_codes {
                println!("{}", code);
            }
        },
        SubCommand::GenerateEthereumAddress(_) => {
            let (private_key, address) = generate_ethereum_address();
            println!(
                "address {:?}; private key {}",
                address, private_key,
            );
        },
    };
}
