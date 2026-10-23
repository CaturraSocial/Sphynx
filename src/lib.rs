pub mod cache;
pub mod config;
pub mod error;
pub mod handlers;
pub mod http_client;
pub mod image_ops;
pub mod ssrf;

use axum::{routing::get, Router};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::handlers::{
    audio_handler, health_handler, image_handler, not_found_handler, root_handler, video_handler,
    AppState,
};

pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        // Health check
        .route("/health", get(health_handler))
        .route("/v1/health", get(health_handler))
        // Unversioned legacy routes
        .route("/", get(root_handler))
        .route("/image", get(image_handler))
        .route("/audio", get(audio_handler))
        .route("/video", get(video_handler))
        // Versioned /v1 routes for future-proofing
        .route("/v1", get(root_handler))
        .route("/v1/", get(root_handler))
        .route("/v1/image", get(image_handler))
        .route("/v1/audio", get(audio_handler))
        .route("/v1/video", get(video_handler))
        .fallback(not_found_handler)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
