pub mod auth;
pub mod bill_view;
pub mod error;
pub mod models;
pub mod routes;
pub mod seed;
pub mod service;
pub mod sim;
pub mod state;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use state::AppState;
use std::str::FromStr;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

/// 建表（幂等）+ 种子数据
pub async fn bootstrap(db_path: &str, with_seed: bool) -> anyhow::Result<AppState> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{db_path}"))?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    init_schema(&pool).await?;
    if with_seed {
        seed::seed(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("seed failed: {e:?}"))?;
    }
    Ok(AppState {
        pool,
        gateway: std::sync::Arc::new(sim::MockGateway::new()),
    })
}

pub async fn init_schema(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let ddl = include_str!("../migrations/0001_init.sql");
    let mut conn = pool.acquire().await?;
    for stmt in ddl.split(';') {
        let s = stmt.trim();
        if !s.is_empty() {
            sqlx::raw_sql(s).execute(&mut *conn).await?;
        }
    }
    Ok(())
}

pub fn app(state: AppState) -> axum::Router {
    let cors = CorsLayer::very_permissive();
    routes::router()
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .fallback_service(ServeDir::new("frontend/dist"))
        .with_state(state)
}
