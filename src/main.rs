use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use sphynx_proxy::{
    build_app,
    cache::DiskCache,
    config::Config,
    handlers::AppState,
    http_client::HttpClient,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sphynx_proxy=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::load();
    let cache = Arc::new(DiskCache::new(&config));
    cache.init().await?;
    cache.clone().start_pruning_task();

    let http_client = HttpClient::new(&config);

    let state = Arc::new(AppState {
        http_client,
        cache,
        config: config.clone(),
        start_time: std::time::Instant::now(),
    });

    let app = build_app(state);

    let addr = format!("{}:{}", config.ip, config.port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Server running at http://{}:{}/", config.ip, config.port);
    info!("Sphynx proxy server listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Sphynx proxy server shut down gracefully.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
