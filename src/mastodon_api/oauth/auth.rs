use actix_web::{
    body::{BodySize, BoxBody, MessageBody},
    dev::ServiceResponse,
    http::StatusCode,
    middleware::{ErrorHandlerResponse, ErrorHandlers},
};
use serde_json::json;
use tokio_postgres::GenericClient;

use crate::database::DatabaseError;
use crate::errors::HttpError;
use crate::models::oauth::queries::get_user_by_oauth_token;
use crate::models::users::types::User;

pub async fn get_current_user(
    db_client: &impl GenericClient,
    token: &str,
) -> Result<User, HttpError> {
    let user = get_user_by_oauth_token(db_client, token).await.map_err(|err| {
        match err {
            DatabaseError::NotFound(_) => {
                HttpError::AuthError("access token is invalid")
            },
            _ => HttpError::InternalError,
        }
    })?;
    Ok(user)
}

/// Error handler for 401 Unauthorized
pub fn create_auth_error_handler<B: MessageBody + 'static>() -> ErrorHandlers<B> {
    ErrorHandlers::new()
        .handler(StatusCode::UNAUTHORIZED, |response: ServiceResponse<B>| {
            let response_new = response.map_body(|_, body| {
                if let BodySize::None | BodySize::Sized(0) = body.size() {
                    // Insert error description if response body is empty
                    // https://github.com/actix/actix-extras/issues/156
                    let error_data = json!({
                        "message": "auth header is not present",
                    });
                    return BoxBody::new(error_data.to_string());
                };
                body.boxed()
            });
            Ok(ErrorHandlerResponse::Response(response_new.map_into_right_body()))
        })
}
