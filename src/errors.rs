use actix_web::{
    http::StatusCode,
    HttpResponse, HttpResponseBuilder,
    error::ResponseError,
};
use serde::Serialize;

use crate::database::DatabaseError;

#[derive(thiserror::Error, Debug)]
#[error("conversion error")]
pub struct ConversionError;

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ValidationError(pub &'static str);

#[derive(thiserror::Error, Debug)]
pub enum HttpError {
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

impl From<DatabaseError> for HttpError {
    fn from(err: DatabaseError) -> Self {
        match err {
            DatabaseError::NotFound(name) => HttpError::NotFoundError(name),
            DatabaseError::AlreadyExists(name) => HttpError::ValidationError(
                format!("{} already exists", name),
            ),
            _ => HttpError::DatabaseError(err),
        }
    }
}

#[derive(Serialize)]
struct ErrorInfo {
    message: String,
}

impl ResponseError for HttpError {
    fn error_response(&self) -> HttpResponse {
        let err = ErrorInfo { message: self.to_string() };
        HttpResponseBuilder::new(self.status_code()).json(err)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            HttpError::ActixError(err) => err.as_response_error().status_code(),
            HttpError::ValidationError(_) => StatusCode::BAD_REQUEST,
            HttpError::ValidationErrorAuto(_) => StatusCode::BAD_REQUEST,
            HttpError::AuthError(_) => StatusCode::UNAUTHORIZED,
            HttpError::PermissionError => StatusCode::FORBIDDEN,
            HttpError::NotFoundError(_) => StatusCode::NOT_FOUND,
            HttpError::NotSupported => StatusCode::IM_A_TEAPOT,
            HttpError::OperationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
