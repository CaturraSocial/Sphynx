use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;
use tracing::error;

use crate::{
    cache::DiskCache,
    config::Config,
    error::ProxyError,
    http_client::{copy_safe_headers, HttpClient},
    image_ops::{self, TargetFormat},
};

pub struct AppState {
    pub http_client: HttpClient,
    pub cache: Arc<DiskCache>,
    pub config: Config,
    pub start_time: std::time::Instant,
}

#[derive(serde::Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub uptime_secs: u64,
    pub cache_size_bytes: u64,
    pub cache_max_size_bytes: u64,
    pub cache_items: usize,
}

pub async fn health_handler(State(state): State<Arc<AppState>>) -> axum::Json<HealthResponse> {
    let (cache_size_bytes, cache_items) = state.cache.stats().await;
    let uptime_secs = state.start_time.elapsed().as_secs();

    axum::Json(HealthResponse {
        status: "ok",
        uptime_secs,
        cache_size_bytes,
        cache_max_size_bytes: state.config.cache_max_size_bytes,
        cache_items,
    })
}

#[derive(Debug, Deserialize, Default)]
pub struct ProxyParams {
    pub url: Option<String>,
    pub format: Option<String>,
    pub w: Option<u32>,
    pub width: Option<u32>,
    pub h: Option<u32>,
    pub height: Option<u32>,
    pub q: Option<u8>,
    pub quality: Option<u8>,
}

const GREETING_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Sphynx Proxy</title>
  <style>
    body { font-family: sans-serif; margin: 40px; line-height: 1.6; color: #222; }
    code { background: #eee; padding: 2px 6px; border-radius: 3px; font-size: 0.9em; }
    li { margin-bottom: 6px; }
  </style>
</head>
<body>
  <h1>Hello, Welcome to the Sphynx Proxy server.</h1>
  <p>To proxy media, pass a target URL using the <code>?url=</code> parameter:</p>
  <ul>
    <li><code>/image?url=&lt;target&gt;</code> or <code>/v1/image?url=&lt;target&gt;</code> (optional: <code>&amp;w=</code>, <code>&amp;h=</code>, <code>&amp;format=webp</code>)</li>
    <li><code>/audio?url=&lt;target&gt;</code> or <code>/v1/audio?url=&lt;target&gt;</code></li>
    <li><code>/video?url=&lt;target&gt;</code> or <code>/v1/video?url=&lt;target&gt;</code></li>
  </ul>
  <p><a href="https://github.com/CaturraSocial/Sphynx">Sphynx on GitHub</a></p>
</body>
</html>"#;

pub async fn root_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ProxyParams>,
) -> Response {
    match params.url.as_deref() {
        None | Some("") => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/html"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            Html(GREETING_HTML),
        )
            .into_response(),
        Some(target) => {
            proxy_media(
                state,
                target,
                &params,
                &headers,
                &["image/", "audio/", "video/"],
            )
            .await
        }
    }
}

pub async fn image_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ProxyParams>,
) -> Response {
    match params.url.as_deref() {
        None | Some("") => {
            ProxyError::InvalidUrl("undefined".to_string()).to_response_with_headers(&headers)
        }
        Some(target) => proxy_media(state, target, &params, &headers, &["image/"]).await,
    }
}

pub async fn audio_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ProxyParams>,
) -> Response {
    match params.url.as_deref() {
        None | Some("") => {
            ProxyError::InvalidUrl("undefined".to_string()).to_response_with_headers(&headers)
        }
        Some(target) => proxy_media(state, target, &params, &headers, &["audio/"]).await,
    }
}

pub async fn video_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ProxyParams>,
) -> Response {
    match params.url.as_deref() {
        None | Some("") => {
            ProxyError::InvalidUrl("undefined".to_string()).to_response_with_headers(&headers)
        }
        Some(target) => proxy_media(state, target, &params, &headers, &["video/"]).await,
    }
}

pub async fn not_found_handler(uri: Uri, headers: HeaderMap) -> Response {
    ProxyError::NotFound(uri.path().to_string()).to_response_with_headers(&headers)
}

async fn proxy_media(
    state: Arc<AppState>,
    target_url: &str,
    params: &ProxyParams,
    headers: &HeaderMap,
    allowed_prefixes: &[&str],
) -> Response {
    let width = params.w.or(params.width);
    let height = params.h.or(params.height);
    let quality = params.q.or(params.quality);

    let format_str = params.format.as_deref().unwrap_or("").to_lowercase();
    let is_raw_requested = format_str == "original" || format_str == "raw";

    let explicit_target_format = if !is_raw_requested {
        TargetFormat::from_str(&format_str)
    } else {
        None
    };

    let client_accepts_webp = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("image/webp"))
        .unwrap_or(false);

    let allows_image = allowed_prefixes.contains(&"image/");

    // Check if image processing should be evaluated
    let should_attempt_image_transcode = allows_image
        && !is_raw_requested
        && (explicit_target_format.is_some()
            || width.is_some()
            || height.is_some()
            || client_accepts_webp);

    let target_format = explicit_target_format.unwrap_or(TargetFormat::WebP);

    if should_attempt_image_transcode {
        let cache_key = DiskCache::generate_key(
            target_url,
            target_format.content_type(),
            width,
            height,
            quality,
        );

        // 1. Check disk cache
        if let Some((meta, file)) = state.cache.get(&cache_key).await {
            return serve_cached_file(meta, file, headers, true).await;
        }

        // 2. Fetch upstream
        let upstream_res = match state
            .http_client
            .fetch(target_url, headers, allowed_prefixes)
            .await
        {
            Ok(res) => res,
            Err(e) => return e.to_response_with_headers(headers),
        };

        let upstream_content_type = upstream_res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // If upstream is NOT image/* (e.g. video on / route), fall back to streaming/caching
        if !upstream_content_type.starts_with("image/") {
            return handle_passthrough_response(state, &target_url, upstream_res, headers).await;
        }

        // Enforce maximum input buffer size
        if let Some(content_length) = upstream_res.content_length() {
            if content_length > state.config.max_image_size_bytes as u64 {
                return ProxyError::PayloadTooLarge(format!(
                    "Image size {content_length} bytes exceeds {} bytes limit",
                    state.config.max_image_size_bytes
                ))
                .to_response_with_headers(headers);
            }
        }

        // Buffer bytes with stream size limit
        let mut byte_stream = upstream_res.bytes_stream();
        let mut raw_bytes = Vec::new();
        let max_size = state.config.max_image_size_bytes;

        while let Some(chunk_res) = byte_stream.next().await {
            match chunk_res {
                Ok(chunk) => {
                    if raw_bytes.len() + chunk.len() > max_size {
                        return ProxyError::PayloadTooLarge(format!(
                            "Image exceeded {max_size} bytes while streaming"
                        ))
                        .to_response_with_headers(headers);
                    }
                    raw_bytes.extend_from_slice(&chunk);
                }
                Err(e) => {
                    return ProxyError::RemoteServer(format!("Failed reading upstream body: {e}"))
                        .to_response_with_headers(headers);
                }
            }
        }

        // Check if image is animated GIF, animated WebP, or SVG: pass through untouched!
        let is_animated = image_ops::is_animated_gif(&raw_bytes)
            || image_ops::is_animated_webp(&raw_bytes)
            || image_ops::is_svg(&raw_bytes, &upstream_content_type);

        if is_animated && explicit_target_format.is_none() && width.is_none() && height.is_none() {
            if raw_bytes.len() <= state.config.cache_max_file_size_bytes {
                let _ = state.cache.put(&cache_key, &upstream_content_type, &raw_bytes).await;
            }

            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, upstream_content_type)
                .header(header::CONTENT_LENGTH, raw_bytes.len().to_string())
                .header(header::CACHE_CONTROL, "public, max-age=86400")
                .header(header::VARY, "Accept")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(Body::from(raw_bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }

        // Process image in blocking thread pool
        let process_res = tokio::task::spawn_blocking(move || {
            image_ops::process_image(&raw_bytes, target_format, width, height, quality)
        })
        .await;

        let (processed_bytes, out_mime) = match process_res {
            Ok(Ok(data)) => data,
            Ok(Err(e)) => return e.to_response_with_headers(headers),
            Err(e) => {
                return ProxyError::Internal(format!("Image task panicked: {e}"))
                    .to_response_with_headers(headers)
            }
        };

        // Cache the processed image
        let etag = match state
            .cache
            .put(&cache_key, out_mime, &processed_bytes)
            .await
        {
            Ok(meta) => meta.etag,
            Err(e) => {
                error!("Failed to store image in disk cache: {e}");
                format!("\"sphynx-{cache_key}\"")
            }
        };

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, out_mime)
            .header(header::CONTENT_LENGTH, processed_bytes.len().to_string())
            .header(header::ETAG, etag)
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .header("X-Cache", "MISS")
            .header(header::VARY, "Accept")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(
                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                "Content-Length, Content-Range, Accept-Ranges, ETag",
            )
            .body(Body::from(processed_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // Pass-through handling for audio, video, or un-transformed media
    let raw_cache_key = DiskCache::generate_key(target_url, "raw", None, None, None);

    if let Some((meta, file)) = state.cache.get(&raw_cache_key).await {
        return serve_cached_file(meta, file, headers, false).await;
    }

    match state
        .http_client
        .fetch(target_url, headers, allowed_prefixes)
        .await
    {
        Ok(upstream_res) => handle_passthrough_response(state, target_url, upstream_res, headers).await,
        Err(e) => e.to_response_with_headers(headers),
    }
}

async fn handle_passthrough_response(
    state: Arc<AppState>,
    target_url: &str,
    upstream_res: reqwest::Response,
    headers: &HeaderMap,
) -> Response {
    let status = upstream_res.status();

    // If partial content (206), stream directly to client
    if status == StatusCode::PARTIAL_CONTENT {
        return stream_upstream_response(upstream_res);
    }

    // If not 200 OK, stream directly
    if status != StatusCode::OK {
        return stream_upstream_response(upstream_res);
    }

    let max_cache_file_size = state.config.cache_max_file_size_bytes;

    // Check if upstream Content-Length exceeds cache limit
    if let Some(content_length) = upstream_res.content_length() {
        if content_length > max_cache_file_size as u64 {
            return stream_upstream_response(upstream_res);
        }
    }

    let upstream_content_type = upstream_res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let mut byte_stream = upstream_res.bytes_stream();
    let mut raw_bytes = Vec::new();

    while let Some(chunk_res) = byte_stream.next().await {
        match chunk_res {
            Ok(chunk) => {
                if raw_bytes.len() + chunk.len() > max_cache_file_size {
                    // Exceeded cache limit while reading -> stream the rest directly
                    let remaining_stream = futures_util::stream::once(async move {
                        Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(raw_bytes))
                    })
                    .chain(futures_util::stream::once(async move {
                        Ok::<bytes::Bytes, std::io::Error>(chunk)
                    }))
                    .chain(byte_stream.map(|res| {
                        res.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                    }));

                    return Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, upstream_content_type)
                        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                        .body(Body::from_stream(remaining_stream))
                        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
                }
                raw_bytes.extend_from_slice(&chunk);
            }
            Err(e) => {
                return ProxyError::RemoteServer(format!("Failed reading upstream body: {e}"))
                    .to_response_with_headers(headers);
            }
        }
    }

    // Cache the downloaded media
    let raw_cache_key = DiskCache::generate_key(target_url, "raw", None, None, None);
    let etag = match state
        .cache
        .put(&raw_cache_key, &upstream_content_type, &raw_bytes)
        .await
    {
        Ok(meta) => meta.etag,
        Err(e) => {
            error!("Failed to store media in disk cache: {e}");
            format!("\"sphynx-{raw_cache_key}\"")
        }
    };

    // If client requested Range, serve 206 Partial Content
    let total_size = raw_bytes.len() as u64;
    if let Some(range_val) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_range(range_val, total_size) {
            let slice = &raw_bytes[start as usize..=end as usize];
            let length = slice.len();

            return Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, upstream_content_type)
                .header(header::CONTENT_LENGTH, length.to_string())
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{total_size}"))
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::ETAG, etag)
                .header(header::CACHE_CONTROL, "public, max-age=86400")
                .header("X-Cache", "MISS")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .header(
                    header::ACCESS_CONTROL_EXPOSE_HEADERS,
                    "Content-Length, Content-Range, Accept-Ranges, ETag",
                )
                .body(Body::from(slice.to_vec()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        } else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{total_size}"))
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, upstream_content_type)
        .header(header::CONTENT_LENGTH, total_size.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header("X-Cache", "MISS")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Length, Content-Range, Accept-Ranges, ETag",
        )
        .body(Body::from(raw_bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn parse_range(range_header: &str, total_size: u64) -> Option<(u64, u64)> {
    if !range_header.starts_with("bytes=") || total_size == 0 {
        return None;
    }
    let spec = &range_header["bytes=".len()..];
    let first = spec.split(',').next()?.trim();
    let mut parts = first.split('-');
    let start_str = parts.next()?.trim();
    let end_str = parts.next()?.trim();

    if start_str.is_empty() {
        let suffix: u64 = end_str.parse().ok()?;
        let start = total_size.saturating_sub(suffix);
        Some((start, total_size - 1))
    } else {
        let start: u64 = start_str.parse().ok()?;
        if start >= total_size {
            return None;
        }
        let end = if end_str.is_empty() {
            total_size - 1
        } else {
            let e: u64 = end_str.parse().ok()?;
            e.min(total_size - 1)
        };
        if start <= end {
            Some((start, end))
        } else {
            None
        }
    }
}

async fn serve_cached_file(
    meta: crate::cache::CacheMeta,
    mut file: tokio::fs::File,
    headers: &HeaderMap,
    vary_accept: bool,
) -> Response {
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if if_none_match.as_bytes() == meta.etag.as_bytes() {
            return StatusCode::NOT_MODIFIED.into_response();
        }
    }

    if let Some(range_val) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        if let Some((start, end)) = parse_range(range_val, meta.size) {
            if file.seek(SeekFrom::Start(start)).await.is_ok() {
                let length = end - start + 1;
                let limited = file.take(length);
                let stream = ReaderStream::new(limited);
                let body = Body::from_stream(stream);

                let mut builder = Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, meta.content_type)
                    .header(header::CONTENT_LENGTH, length.to_string())
                    .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{}", meta.size))
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::ETAG, meta.etag)
                    .header(header::CACHE_CONTROL, "public, max-age=86400")
                    .header("X-Cache", "HIT")
                    .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .header(
                        header::ACCESS_CONTROL_EXPOSE_HEADERS,
                        "Content-Length, Content-Range, Accept-Ranges, ETag",
                    );

                if vary_accept {
                    builder = builder.header(header::VARY, "Accept");
                }

                return builder.body(body).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        } else {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{}", meta.size))
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    }

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, meta.content_type)
        .header(header::CONTENT_LENGTH, meta.size.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ETAG, meta.etag)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header("X-Cache", "HIT")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Length, Content-Range, Accept-Ranges, ETag",
        );

    if vary_accept {
        builder = builder.header(header::VARY, "Accept");
    }

    builder.body(body).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn stream_upstream_response(upstream_res: reqwest::Response) -> Response {
    let status = upstream_res.status();
    let upstream_headers = upstream_res.headers().clone();
    let mut out_headers = HeaderMap::new();

    copy_safe_headers(&upstream_headers, &mut out_headers);

    let stream = upstream_res
        .bytes_stream()
        .map(|res| res.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

    let body = Body::from_stream(stream);

    let mut response = Response::builder().status(status);
    for (k, v) in out_headers {
        if let Some(key) = k {
            response = response.header(key, v);
        }
    }

    response
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_scenarios() {
        let total = 1000;

        // Exact range
        assert_eq!(parse_range("bytes=0-499", total), Some((0, 499)));

        // From offset to end
        assert_eq!(parse_range("bytes=500-", total), Some((500, 999)));

        // Suffix range (last N bytes)
        assert_eq!(parse_range("bytes=-200", total), Some((800, 999)));

        // Start beyond EOF -> None (causes 416)
        assert_eq!(parse_range("bytes=1500-", total), None);

        // Clamping end beyond EOF
        assert_eq!(parse_range("bytes=800-2000", total), Some((800, 999)));

        // Invalid headers
        assert_eq!(parse_range("characters=0-500", total), None);
        assert_eq!(parse_range("invalid", total), None);
    }
}
