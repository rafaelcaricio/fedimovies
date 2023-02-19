use actix_web::{
    body::{BodySize, BoxBody, MessageBody},
    dev::ServiceResponse,
    error::{Error, JsonPayloadError},
    http::StatusCode,
    middleware::{ErrorHandlerResponse, ErrorHandlers},
    HttpRequest,
};
use serde_json::json;

use crate::errors::HttpError;

/// Error handler for 401 Unauthorized
pub fn create_auth_error_handler<B: MessageBody + 'static>() -> ErrorHandlers<B> {
    // Creates and returns actix middleware
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

pub fn json_error_handler(
    error: JsonPayloadError,
    _: &HttpRequest,
) -> Error {
    match error {
        JsonPayloadError::Deserialize(de_error) => {
            HttpError::ValidationError(de_error.to_string()).into()
        },
        other_error => other_error.into(),
    }
}
