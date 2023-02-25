use actix_web::{
    error::ResponseError,
    http::StatusCode,
    HttpResponse,
    HttpResponseBuilder,
};
use serde::Serialize;

use crate::database::DatabaseError;
use crate::errors::ValidationError;

#[derive(thiserror::Error, Debug)]
pub enum MastodonError {
    #[error(transparent)]
    ActixError(#[from] actix_web::Error),

    #[error("database error")]
    DatabaseError(#[source] DatabaseError),

    #[error("{0}")]
    ValidationError(String),

    #[error("{0}")]
    ValidationErrorAuto(#[from] ValidationError),

    #[error("{0}")]
    AuthError(&'static str),

    #[error("permission error")]
    PermissionError,

    #[error("{0} not found")]
    NotFoundError(&'static str),

    #[error("operation not supported")]
    NotSupported,

    #[error("{0}")]
    OperationError(&'static str),

    #[error("internal error")]
    InternalError,
}

impl From<DatabaseError> for MastodonError {
    fn from(error: DatabaseError) -> Self {
        match error {
            DatabaseError::NotFound(name) => Self::NotFoundError(name),
            DatabaseError::AlreadyExists(name) => Self::ValidationError(
                format!("{} already exists", name),
            ),
            _ => Self::DatabaseError(error),
        }
    }
}

/// https://docs.joinmastodon.org/entities/Error/
#[derive(Serialize)]
struct MastodonErrorData {
    message: String, // deprecated
    error: String,
    error_description: Option<String>,
}

impl ResponseError for MastodonError {
    fn error_response(&self) -> HttpResponse {
        let error_data = MastodonErrorData {
            message: self.to_string(),
            error: self.to_string(),
            error_description: Some(self.to_string()),
        };
        HttpResponseBuilder::new(self.status_code()).json(error_data)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::ActixError(error) =>
                error.as_response_error().status_code(),
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::ValidationErrorAuto(_) => StatusCode::BAD_REQUEST,
            Self::AuthError(_) => StatusCode::UNAUTHORIZED,
            Self::PermissionError => StatusCode::FORBIDDEN,
            Self::NotFoundError(_) => StatusCode::NOT_FOUND,
            Self::NotSupported => StatusCode::IM_A_TEAPOT,
            Self::OperationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
