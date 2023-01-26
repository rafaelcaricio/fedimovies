use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::config::Config;
use crate::database::{
    get_database_client,
    DatabaseClient,
    DatabaseError,
    DatabaseTypeError,
    DbPool,
};
use crate::models::{
    background_jobs::queries::{
        enqueue_job,
        get_job_batch,
        delete_job_from_queue,
    },
    background_jobs::types::JobType,
    users::queries::get_user_by_id,
};
use super::deliverer::{OutgoingActivity, Recipient};
use super::fetcher::fetchers::FetchError;
use super::receiver::{handle_activity, HandlerError};

#[derive(Deserialize, Serialize)]
pub struct IncomingActivityJobData {
    activity: Value,
    is_authenticated: bool,
    failure_count: i32,
}

impl IncomingActivityJobData {
    pub fn new(activity: &Value, is_authenticated: bool) -> Self {
        Self {
            activity: activity.clone(),
            is_authenticated,
            failure_count: 0,
        }
    }

    pub async fn into_job(
        self,
        db_client: &impl DatabaseClient,
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

pub async fn process_queued_incoming_activities(
    config: &Config,
    db_client: &mut impl DatabaseClient,
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
        let mut job_data: IncomingActivityJobData =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        if let Err(error) = handle_activity(
            config,
            db_client,
            &job_data.activity,
            job_data.is_authenticated,
        ).await {
            job_data.failure_count += 1;
            log::warn!(
                "failed to process activity ({}) (attempt #{}): {}",
                error,
                job_data.failure_count,
                job_data.activity,
            );
            if job_data.failure_count <= max_retries &&
                // Don't retry after fetcher recursion error
                !matches!(error, HandlerError::FetchError(FetchError::RecursionError))
            {
                // Re-queue
                log::info!("activity re-queued");
                job_data.into_job(db_client, retry_after).await?;
            };
        };
        delete_job_from_queue(db_client, &job.id).await?;
    };
    Ok(())
}

#[derive(Deserialize, Serialize)]
pub struct OutgoingActivityJobData {
    pub activity: Value,
    pub sender_id: Uuid,
    pub recipients: Vec<Recipient>,
}

impl OutgoingActivityJobData {
    pub async fn into_job(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        let job_data = serde_json::to_value(self)
            .expect("activity should be serializable");
        let scheduled_for = Utc::now();
        enqueue_job(
            db_client,
            &JobType::OutgoingActivity,
            &job_data,
            &scheduled_for,
        ).await
    }
}

pub async fn process_queued_outgoing_activities(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), DatabaseError> {
    let db_client = &**get_database_client(db_pool).await?;
    let batch_size = 1;
    let batch = get_job_batch(
        db_client,
        &JobType::OutgoingActivity,
        batch_size,
    ).await?;
    for job in batch {
        let job_data: OutgoingActivityJobData =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        let sender = get_user_by_id(db_client, &job_data.sender_id).await?;
        let outgoing_activity = OutgoingActivity {
            db_pool: Some(db_pool.clone()),
            instance: config.instance(),
            sender,
            activity: job_data.activity,
            recipients: job_data.recipients,
        };
        outgoing_activity.spawn_deliver();
        delete_job_from_queue(db_client, &job.id).await?;
    };
    Ok(())
}
