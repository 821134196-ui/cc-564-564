use crate::handlers;
use axum::{
    routing::{get, post},
    Router,
};
use sqlx::SqlitePool;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

pub fn router(pool: SqlitePool) -> Router {
    let state = AppState { pool };

    let api = Router::new()
        .route("/auth/login", post(handlers::login))
        .route("/auth/me", get(handlers::me))
        .route("/meta", get(handlers::meta))
        .route("/bills", get(handlers::list_bills).post(handlers::create_bill))
        .route("/bills/{id}", get(handlers::get_bill))
        .route("/bills/{id}/audit", get(handlers::bill_audit))
        .route("/bills/{id}/refunds", post(handlers::create_refund))
        .route("/bills/{id}/refunds/preview", post(handlers::preview_refund))
        .route("/items", get(handlers::list_items))
        .route("/items/{id}/execute", post(handlers::execute_item))
        .route("/items/{id}/flag", post(handlers::flag_item))
        .route("/items/{id}/report", post(handlers::issue_report))
        .route("/reports", get(handlers::list_reports))
        .route("/reports/{id}/void", post(handlers::void_report))
        .route("/refunds", get(handlers::list_refunds))
        .route("/refunds/{id}", get(handlers::get_refund))
        .route("/patients", get(handlers::list_patients))
        .route("/packages", get(handlers::list_packages))
        .with_state(state);

    // 生产模式：后端直接托管前端构建产物
    let static_dir = ServeDir::new("frontend/dist")
        .not_found_service(ServeFile::new("frontend/dist/index.html"));

    Router::new().nest("/api", api).fallback_service(static_dir)
}
