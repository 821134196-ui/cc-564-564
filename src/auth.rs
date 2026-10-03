use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Doctor,
    Cashier,
    ReportStaff,
    Patient,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Doctor => "DOCTOR",
            Role::Cashier => "CASHIER",
            Role::ReportStaff => "REPORT_STAFF",
            Role::Patient => "PATIENT",
        }
    }
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "DOCTOR" => Some(Role::Doctor),
            "CASHIER" => Some(Role::Cashier),
            "REPORT_STAFF" => Some(Role::ReportStaff),
            "PATIENT" => Some(Role::Patient),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub role: String,
    pub token: String,
    pub patient_id: Option<String>,
}

impl User {
    pub fn role(&self) -> Role {
        Role::parse(&self.role).expect("user role valid")
    }
}

/// 从请求头 `Authorization: Bearer <token>` 解析当前登录用户
pub struct AuthUser(pub User);

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| AppError::Unauthorized("缺少 Authorization: Bearer <token>".into()))?;

        let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE token = ?1")
            .bind(header)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::Unauthorized("令牌无效".into()))?;
        Ok(AuthUser(user))
    }
}

macro_rules! require_role {
    ($name:ident, $($role:ident),+) => {
        pub struct $name(pub User);

        #[async_trait]
        impl FromRequestParts<AppState> for $name {
            type Rejection = AppError;
            async fn from_request_parts(parts: &mut Parts, state: &AppState) -> AppResult<Self> {
                let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
                const ALLOWED: &[Role] = &[$(Role::$role),+];
                if !ALLOWED.contains(&user.role()) {
                    return Err(AppError::Forbidden(format!(
                        "岗位 {} 无权执行该操作（允许：{}）",
                        user.role,
                        ALLOWED.iter().map(|r| r.as_str()).collect::<Vec<_>>().join("/")
                    )));
                }
                Ok($name(user))
            }
        }
    };
}

require_role!(RequireDoctor, Doctor);
require_role!(RequireCashier, Cashier);
require_role!(RequireReportStaff, ReportStaff);
require_role!(RequireStaff, Doctor, Cashier, ReportStaff);
