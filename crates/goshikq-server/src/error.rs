use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use goshikq_runtime::RuntimeError;
use goshikq_storage::StorageError;
use serde::Serialize;
use utoipa::ToSchema;

/// Consistent error envelope for every endpoint. Messages never include secrets or
/// internal SQL.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError {
            status,
            code,
            message: message.into(),
            details: None,
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(code = self.code, message = %self.message, "request failed");
        }
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code.to_string(),
                message: self.message,
                details: self.details,
            },
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self {
        match e {
            StorageError::NotFound(m) => ApiError::not_found(m),
            StorageError::Conflict(m) => ApiError::conflict(m),
            StorageError::Json(e) => ApiError::bad_request(e.to_string()),
            other => {
                tracing::error!(error = %other, "storage error");
                ApiError::internal("storage error")
            }
        }
    }
}

impl From<RuntimeError> for ApiError {
    fn from(e: RuntimeError) -> Self {
        match e {
            RuntimeError::Storage(s) => s.into(),
            RuntimeError::NotActive(id) => {
                ApiError::conflict(format!("run {id} is not active on this runtime"))
            }
            RuntimeError::Invalid(m) => ApiError::bad_request(m),
            RuntimeError::Conflict(m) => ApiError::conflict(m),
        }
    }
}
