use actix_web::{
    HttpRequest,
    error::{Error, JsonPayloadError},
};

use crate::errors::HttpError;

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
