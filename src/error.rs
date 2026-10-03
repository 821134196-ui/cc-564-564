use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    /// 422 业务规则拦截（不可退、重复退项、越权状态变更等）
    Business(String),
    Sqlx(sqlx::Error),
}

impl AppError {
    pub fn business(msg: impl Into<String>) -> Self {
        AppError::Business(msg.into())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Sqlx(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", m),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, "FORBIDDEN", m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "NOT_FOUND", m),
            AppError::Business(m) => (StatusCode::UNPROCESSABLE_ENTITY, "BUSINESS_RULE", m),
            AppError::Sqlx(e) => {
                tracing::error!("database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL",
                    "数据库错误".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
