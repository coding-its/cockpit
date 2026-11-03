use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Upstream(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, message),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            ApiError::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::Upstream(message) => (StatusCode::BAD_GATEWAY, message),
            ApiError::Database(error) => {
                tracing::error!(?error, "Datenbankfehler");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Interner Datenbankfehler".to_string(),
                )
            }
            ApiError::Other(error) => {
                tracing::error!(?error, "Unerwarteter Fehler");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Interner Fehler".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
