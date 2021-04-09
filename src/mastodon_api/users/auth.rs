use actix_session::Session;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::HttpError;
use crate::models::users::queries::get_user_by_id;
use crate::models::users::types::User;

pub async fn get_current_user(
    db_client: &impl GenericClient,
    session: Session,
) -> Result<User, HttpError> {
    let maybe_user_id = session.get::<String>("id")
        .map_err(|_| HttpError::SessionError("failed to read cookie"))?;
    if let Some(user_id) = maybe_user_id {
        let user_uuid = Uuid::parse_str(&user_id)
            .map_err(|_| HttpError::SessionError("invalid uuid"))?;
        let user = get_user_by_id(db_client, &user_uuid)
            .await
            .map_err(|_| HttpError::SessionError("user not found"))?;
        Ok(user)
    } else {
        return Err(HttpError::SessionError("session not found"));
    }
}
