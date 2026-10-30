use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Application error that converts into a JSON HTTP response.
pub enum AppError {
    NotFound,
    PayloadTooLarge,
    BadRequest(String),
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            AppError::PayloadTooLarge => {
                (StatusCode::PAYLOAD_TOO_LARGE, "file too large".to_string())
            }
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Internal(err) => {
                tracing::error!("internal error: {err:?}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

// Lets `?` turn any error that converts into anyhow::Error (sqlx, io, multipart…)
// into a 500. Specific client-error cases are constructed explicitly above.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        AppError::Internal(err.into())
    }
}
