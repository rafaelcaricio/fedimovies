use crate::database::Pool;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

pub async fn apply_migrations(pool: &Pool) {
    // https://github.com/rust-db/refinery/issues/105
    let mut client_object = pool.get().await.unwrap();
    let client = &mut *(*client_object);
    let migration_report = embedded::migrations::runner()
        .run_async(client)
        .await.unwrap();

    for migration in migration_report.applied_migrations() {
        log::info!(
            "Migration Applied -  Name: {}, Version: {}",
            migration.name(),
            migration.version(),
        );
    }
}
