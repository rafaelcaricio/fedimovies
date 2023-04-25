use tokio_postgres::Client;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub async fn apply_migrations(db_client: &mut Client) {
    let migration_report = embedded::migrations::runner()
        .run_async(db_client)
        .await
        .unwrap();

    for migration in migration_report.applied_migrations() {
        log::info!(
            "migration applied: version {} ({})",
            migration.version(),
            migration.name(),
        );
    }
}
