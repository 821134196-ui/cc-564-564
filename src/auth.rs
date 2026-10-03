use crate::models::{SkippedItem, UserInfo};
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Doctor,
    Cashier,
    Reporter,
    Admin,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "doctor" => Some(Role::Doctor),
            "cashier" => Some(Role::Cashier),
            "reporter" => Some(Role::Reporter),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Doctor => "doctor",
            Role::Cashier => "cashier",
            Role::Reporter => "reporter",
            Role::Admin => "admin",
        }
    }
}

/// 已登录用户（从 Authorization: Bearer <token> 解析）
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: Role,
}

impl AuthUser {
    pub fn info(&self) -> UserInfo {
        UserInfo {
            id: self.id,
            username: self.username.clone(),
            display_name: self.display_name.clone(),
            role: self.role.as_str().to_string(),
        }
    }

    /// 岗位权限校验：不在允许列表中则 403
    pub fn require(&self, allowed: &[Role]) -> Result<(), ApiError> {
        if allowed.contains(&self.role) {
            Ok(())
        } else {
            Err(ApiError::Forbidden(format!(
                "岗位越权：当前角色 {} 无权执行此操作",
                self.role.as_str()
            )))
        }
    }
}

impl FromRequestParts<crate::app::AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::app::AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::Unauthorized("未登录：缺少 Authorization 头".into()))?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ApiError::Unauthorized("未登录：凭证格式错误".into()))?;

        let row: Option<(i64, String, String, String)> = sqlx::query_as(
            "SELECT u.id, u.username, u.display_name, u.role
             FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token = ?",
        )
        .bind(token)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

        let (id, username, display_name, role) =
            row.ok_or_else(|| ApiError::Unauthorized("未登录或会话已失效".into()))?;
        let role = Role::parse(&role).ok_or_else(|| ApiError::Internal("未知角色".into()))?;
        Ok(AuthUser {
            id,
            username,
            display_name,
            role,
        })
    }
}

/// 统一 API 错误：JSON {error, code, skipped?}
#[derive(Debug)]
pub enum ApiError {
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    /// 全部项目均不可退等业务冲突，可附带逐项原因
    Conflict(String, Vec<SkippedItem>),
    Unprocessable(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg, skipped) = match self {
            ApiError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m, None),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, m, None),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m, None),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m, None),
            ApiError::Conflict(m, s) => (StatusCode::CONFLICT, m, Some(s)),
            ApiError::Unprocessable(m) => (StatusCode::UNPROCESSABLE_ENTITY, m, None),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m, None),
        };
        let mut body = json!({ "error": msg, "code": status.as_u16() });
        if let Some(skipped) = skipped {
            body["skipped"] = serde_json::to_value(skipped).unwrap_or_default();
        }
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Internal(format!("数据库错误: {e}"))
    }
}
