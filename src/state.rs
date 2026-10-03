use sqlx::sqlite::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    /// 保险/支付模拟网关
    pub gateway: Arc<crate::sim::MockGateway>,
}
