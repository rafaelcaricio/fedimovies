use actix_web::{
    body::{Body, BodySize, MessageBody, ResponseBody},
    http::StatusCode,
    middleware::errhandlers::{ErrorHandlerResponse, ErrorHandlers},
};
use actix_web::dev::ServiceResponse;
use serde_json::json;
use tokio_postgres::GenericClient;

use crate::errors::{DatabaseError, HttpError};
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
pub fn create_auth_error_handler<B: MessageBody>() -> ErrorHandlers<B> {
    ErrorHandlers::new()
        .handler(StatusCode::UNAUTHORIZED, |mut response: ServiceResponse<B>| {
            response = response.map_body(|_, body| {
                if let ResponseBody::Body(data) = &body {
                    if let BodySize::Empty = data.size() {
                        // Insert error description if response body is empty
                        // https://github.com/actix/actix-extras/issues/156
                        let error_data = json!({
                            "message": "auth header is not present",
                        });
                        return ResponseBody::Body(Body::from(error_data)).into_body();
                    }
                }
                body
            });
            Ok(ErrorHandlerResponse::Response(response))
        })
}
