use clap::Clap;
use uuid::Uuid;

use mitra::config;
use mitra::database::create_database_client;
use mitra::database::migrate::apply_migrations;
use mitra::ethereum::utils::generate_ethereum_address;
use mitra::logger::configure_logger;
use mitra::models::posts::queries::delete_post;
use mitra::models::profiles::queries::delete_profile;
use mitra::models::users::queries::{
    create_invite_code,
    get_invite_codes,
};
use mitra::utils::crypto::{generate_private_key, serialize_private_key};

/// Admin CLI tool
#[derive(Clap)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    GenerateRsaKey(GenerateRsaKey),
    GenerateEthereumAddress(GenerateEthereumAddress),

    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    DeleteProfile(DeleteProfile),
    DeletePost(DeletePost),
}

/// Generate RSA private key
#[derive(Clap)]
struct GenerateRsaKey;

impl GenerateRsaKey {
    fn execute(&self) -> () {
        let private_key = generate_private_key().unwrap();
        let private_key_str = serialize_private_key(private_key).unwrap();
        println!("{}", private_key_str);
    }
}

/// Generate ethereum address
#[derive(Clap)]
struct GenerateEthereumAddress;

/// Generate invite code
#[derive(Clap)]
struct GenerateInviteCode;

/// List invite codes
#[derive(Clap)]
struct ListInviteCodes;

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

#[tokio::main]
async fn main() {
    let opts: Opts = Opts::parse();

    match opts.subcmd {
        SubCommand::GenerateRsaKey(cmd) => cmd.execute(),
        SubCommand::GenerateEthereumAddress(_) => {
            let (private_key, address) = generate_ethereum_address();
            println!(
                "address {:?}; private key {}",
                address, private_key,
            );
        },
        subcmd => {
            // Other commands require initialized app
            let config = config::parse_config();
            configure_logger(config.log_level);
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
                    }
                    for code in invite_codes {
                        println!("{}", code);
                    }
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
                _ => panic!(),
            };
        },
    };
}
