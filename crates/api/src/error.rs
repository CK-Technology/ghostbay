use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Bucket not found: {0}")]
    BucketNotFound(String),
    
    #[error("Object not found: {0}")]
    ObjectNotFound(String),
    
    #[error("Bucket already exists: {0}")]
    BucketAlreadyExists(String),
    
    #[error("Invalid bucket name: {0}")]
    InvalidBucketName(String),
    
    #[error("Invalid object key: {0}")]
    InvalidObjectKey(String),
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),
    
    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Invalid request: {0}")]
    BadRequest(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            ApiError::BucketNotFound(_) => (StatusCode::NOT_FOUND, "NoSuchBucket", self.to_string()),
            ApiError::ObjectNotFound(_) => (StatusCode::NOT_FOUND, "NoSuchKey", self.to_string()),
            ApiError::BucketAlreadyExists(_) => (StatusCode::CONFLICT, "BucketAlreadyExists", self.to_string()),
            ApiError::InvalidBucketName(_) => (StatusCode::BAD_REQUEST, "InvalidBucketName", self.to_string()),
            ApiError::InvalidObjectKey(_) => (StatusCode::BAD_REQUEST, "InvalidObjectKey", self.to_string()),
            ApiError::AuthenticationFailed(_) => (StatusCode::UNAUTHORIZED, "AccessDenied", self.to_string()),
            ApiError::AuthorizationFailed(_) => (StatusCode::FORBIDDEN, "AccessDenied", self.to_string()),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "InvalidRequest", self.to_string()),
            ApiError::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Storage operation failed".to_string()),
            ApiError::Internal(_) | ApiError::Database(_) => {
                tracing::error!("Internal error: {}", self);
                (StatusCode::INTERNAL_SERVER_ERROR, "InternalError", "Internal server error".to_string())
            }
        };

        let body = Json(json!({
            "Code": error_code,
            "Message": message,
            "RequestId": "00000000-0000-0000-0000-000000000000", // TODO: Add proper request ID
        }));

        (status, body).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;