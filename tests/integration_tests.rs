use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;

use sphynx_proxy::{
    build_app,
    cache::DiskCache,
    config::Config,
    handlers::AppState,
    http_client::HttpClient,
};

fn create_test_state() -> Arc<AppState> {
    let temp_dir = std::env::temp_dir().join(format!("sphynx_integ_test_{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos()));
    let mut config = Config::load();
    config.cache_dir = temp_dir;
    config.cache_ttl = Duration::from_secs(3600);
    config.cache_max_size_bytes = 10_000_000;

    let cache = Arc::new(DiskCache::new(&config));
    let http_client = HttpClient::new(&config);

    Arc::new(AppState {
        http_client,
        cache,
        config,
        start_time: std::time::Instant::now(),
    })
}

#[tokio::test]
async fn test_health_endpoint() {
    let state = create_test_state();
    let app = build_app(state);

    for route in ["/health", "/v1/health"] {
        let req = Request::builder()
            .uri(route)
            .method("GET")
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["status"], "ok");
        assert!(json["uptime_secs"].is_number());
        assert!(json["cache_size_bytes"].is_number());
        assert!(json["cache_max_size_bytes"].is_number());
        assert!(json["cache_items"].is_number());
    }
}

#[tokio::test]
async fn test_root_and_v1_greeting() {
    let state = create_test_state();
    let app = build_app(state.clone());

    let routes = ["/", "/v1", "/v1/"];
    for route in routes {
        let req = Request::builder()
            .uri(route)
            .method("GET")
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(header::CONTENT_TYPE).unwrap(), "text/html");

        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes);
        assert!(body_str.contains("Hello, Welcome to the Sphynx Proxy server."));
        assert!(body_str.contains("https://github.com/CaturraSocial/Sphynx"));
    }
}

#[tokio::test]
async fn test_missing_url_parameter_on_subroutes() {
    let state = create_test_state();
    let app = build_app(state);

    let endpoints = ["/image", "/v1/image", "/audio", "/v1/audio", "/video", "/v1/video"];
    for endpoint in endpoints {
        let req = Request::builder()
            .uri(endpoint)
            .method("GET")
            .body(Body::empty())
            .unwrap();

        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes);
        assert!(body_str.contains("400 Bad Request: Unable to parse URL: undefined"));
    }
}

#[tokio::test]
async fn test_missing_url_with_json_accept_header() {
    let state = create_test_state();
    let app = build_app(state);

    let req = Request::builder()
        .uri("/v1/image")
        .header(header::ACCEPT, "application/json")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json_val: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(json_val["status"], 400);
    assert_eq!(json_val["error"], "Unable to parse URL: undefined");
}

#[tokio::test]
async fn test_not_found_routes() {
    let state = create_test_state();
    let app = build_app(state);

    let req = Request::builder()
        .uri("/nonexistent-path")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(String::from_utf8_lossy(&body_bytes), "404 Not Found");
}

#[tokio::test]
async fn test_ssrf_blocking_integration() {
    let state = create_test_state();
    let app = build_app(state);

    let req = Request::builder()
        .uri("/image?url=http://127.0.0.1:8080/secret.png")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert!(body_str.contains("SSRF protection"));
}

#[tokio::test]
async fn test_cors_preflight() {
    let state = create_test_state();
    let app = build_app(state);

    let req = Request::builder()
        .uri("/v1/image")
        .method("OPTIONS")
        .header(header::ORIGIN, "https://example.com")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "*"
    );
}
