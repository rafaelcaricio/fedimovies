use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};
use super::types::{DbBackgroundJob, JobStatus, JobType};

pub async fn enqueue_job(
    db_client: &impl DatabaseClient,
    job_type: &JobType,
    job_data: &Value,
    scheduled_for: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    let job_id = Uuid::new_v4();
    db_client.execute(
        "
        INSERT INTO background_job (
            id,
            job_type,
            job_data,
            scheduled_for
        )
        VALUES ($1, $2, $3, $4)
        ",
        &[&job_id, &job_type, &job_data, &scheduled_for],
    ).await?;
    Ok(())
}

pub async fn get_job_batch(
    db_client: &impl DatabaseClient,
    job_type: &JobType,
    batch_size: u32,
    job_timeout: u32,
) -> Result<Vec<DbBackgroundJob>, DatabaseError> {
    // https://github.com/sfackler/rust-postgres/issues/60
    let job_timeout_pg = format!("{}S", job_timeout); // interval
    let rows = db_client.query(
        "
        UPDATE background_job
        SET
            job_status = $1,
            updated_at = CURRENT_TIMESTAMP
        WHERE id IN (
            SELECT id
            FROM background_job
            WHERE
                job_type = $2
                AND scheduled_for < CURRENT_TIMESTAMP
                AND (
                    job_status = $3 --queued
                    OR job_status = $1 --running
                    AND updated_at < CURRENT_TIMESTAMP - $5::text::interval
                )
            ORDER BY scheduled_for ASC
            LIMIT $4
        )
        RETURNING background_job
        ",
        &[
            &JobStatus::Running,
            &job_type,
            &JobStatus::Queued,
            &i64::from(batch_size),
            &job_timeout_pg,
        ],
    ).await?;
    let jobs = rows.iter()
        .map(|row| row.try_get("background_job"))
        .collect::<Result<_, _>>()?;
    Ok(jobs)
}

pub async fn delete_job_from_queue(
    db_client: &impl DatabaseClient,
    job_id: &Uuid,
) -> Result<(), DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM background_job
        WHERE id = $1
        ",
        &[&job_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("background job"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_queue() {
        let db_client = &create_test_database().await;
        let job_type = JobType::IncomingActivity;
        let job_data = json!({
            "activity": {},
            "is_authenticated": true,
            "failure_count": 0,
        });
        let scheduled_for = Utc::now();
        enqueue_job(db_client, &job_type, &job_data, &scheduled_for).await.unwrap();

        let batch_1 = get_job_batch(db_client, &job_type, 10, 3600).await.unwrap();
        assert_eq!(batch_1.len(), 1);
        let job = &batch_1[0];
        assert_eq!(job.job_type, job_type);
        assert_eq!(job.job_data, job_data);
        assert_eq!(job.job_status, JobStatus::Running);

        let batch_2 = get_job_batch(db_client, &job_type, 10, 3600).await.unwrap();
        assert_eq!(batch_2.len(), 0);

        delete_job_from_queue(db_client, &job.id).await.unwrap();
        let batch_3 = get_job_batch(db_client, &job_type, 10, 3600).await.unwrap();
        assert_eq!(batch_3.len(), 0);
    }
}
