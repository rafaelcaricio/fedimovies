use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::config::Config;
use crate::database::{DatabaseError, DatabaseTypeError};
use crate::models::{
    background_jobs::queries::{
        enqueue_job,
        get_job_batch,
        delete_job_from_queue,
    },
    background_jobs::types::JobType,
};
use super::receiver::handle_activity;

#[derive(Deserialize, Serialize)]
pub struct IncomingActivity {
    activity: Value,
    is_authenticated: bool,
    failure_count: i32,
}

impl IncomingActivity {
    pub fn new(activity: &Value, is_authenticated: bool) -> Self {
        Self {
            activity: activity.clone(),
            is_authenticated,
            failure_count: 0,
        }
    }

    pub async fn enqueue(
        self,
        db_client: &impl GenericClient,
        delay: i64,
    ) -> Result<(), DatabaseError> {
        let job_data = serde_json::to_value(self)
            .expect("activity should be serializable");
        let scheduled_for = Utc::now() + Duration::seconds(delay);
        enqueue_job(
            db_client,
            &JobType::IncomingActivity,
            &job_data,
            &scheduled_for,
        ).await
    }
}

pub async fn process_queued_activities(
    config: &Config,
    db_client: &mut impl GenericClient,
) -> Result<(), DatabaseError> {
    let batch_size = 10;
    let max_retries = 2;
    let retry_after = 60 * 10; // 10 minutes

    let batch = get_job_batch(
        db_client,
        &JobType::IncomingActivity,
        batch_size,
    ).await?;
    for job in batch {
        let mut incoming_activity: IncomingActivity =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        let is_error = match handle_activity(
            config,
            db_client,
            &incoming_activity.activity,
            incoming_activity.is_authenticated,
        ).await {
            Ok(_) => false,
            Err(error) => {
                incoming_activity.failure_count += 1;
                log::warn!(
                    "failed to process activity ({}) (attempt #{}): {}",
                    error,
                    incoming_activity.failure_count,
                    incoming_activity.activity,
                );
                true
            },
        };
        if is_error && incoming_activity.failure_count <= max_retries {
            // Re-queue
            log::info!("activity re-queued");
            incoming_activity.enqueue(db_client, retry_after).await?;
        };
        delete_job_from_queue(db_client, &job.id).await?;
    };
    Ok(())
}
