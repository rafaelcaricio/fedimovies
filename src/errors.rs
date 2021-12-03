use actix_web::{
    dev::HttpResponseBuilder,
    http::StatusCode,
    HttpResponse,
    error::ResponseError,
};
use serde::Serialize;

#[derive(thiserror::Error, Debug)]
#[error("conversion error")]
pub struct ConversionError;

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ValidationError(pub &'static str);

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error("database pool error")]
    DatabasePoolError(#[from] deadpool_postgres::PoolError),

    #[error("database client error")]
    DatabaseClientError(#[from] tokio_postgres::Error),

    #[error("database type error")]
    DatabaseTypeError(#[from] ConversionError),

    #[error("{0}")]
    NotFound(&'static str), // object type

    #[error("{0}")]
    AlreadyExists(&'static str), // object type
}

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
    SessionError(&'static str),

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
            HttpError::SessionError(_) => StatusCode::UNAUTHORIZED,
            HttpError::PermissionError => StatusCode::FORBIDDEN,
            HttpError::NotFoundError(_) => StatusCode::NOT_FOUND,
            HttpError::NotSupported => StatusCode::IM_A_TEAPOT,
            HttpError::OperationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
