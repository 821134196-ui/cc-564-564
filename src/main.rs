use clinic_refund::{app, db};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/clinic.db".into());
    if let Some(dir) = db_url.strip_prefix("sqlite://").and_then(|p| {
        let path = std::path::Path::new(p);
        path.parent().filter(|d| !d.as_os_str().is_empty()).map(|d| d.to_path_buf())
    }) {
        std::fs::create_dir_all(&dir).ok();
    }

    let pool = db::init_pool(&db_url).await.expect("数据库初始化失败");
    db::seed(&pool).await.expect("种子数据写入失败");

    let app = app::router(pool);
    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:3000".into());
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("端口绑定失败");
    tracing::info!("诊所退项与费用调整系统已启动: http://localhost:{}", bind.replace("0.0.0.0", "127.0.0.1").split(':').last().unwrap_or("3000"));
    axum::serve(listener, app).await.expect("服务异常退出");
}
