use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use reqwest::Response as ReqwestResponse;

use crate::{config::Config, error::ProxyError, ssrf};

#[derive(Clone)]
pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new(config: &Config) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none()) // Handle redirects manually for SSRF checking
            .connect_timeout(config.connect_timeout)
            .timeout(config.read_timeout)
            .build()
            .expect("Failed to build reqwest client");

        Self { client }
    }

    pub async fn fetch(
        &self,
        raw_url: &str,
        client_headers: &HeaderMap,
        allowed_mime_prefixes: &[&str],
    ) -> Result<ReqwestResponse, ProxyError> {
        let mut current_url = ssrf::validate_url(raw_url).await?;
        let mut redirects_made = 0;
        const MAX_REDIRECTS: usize = 5;

        loop {
            let mut req = self
                .client
                .get(current_url.as_str())
                .header(header::USER_AGENT, "Sphynx-Proxy/2.0");

            // Forward Range header if present
            if let Some(range_val) = client_headers.get(header::RANGE) {
                req = req.header(header::RANGE, range_val);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| ProxyError::RemoteServer(e.to_string()))?;

            let status = resp.status();

            // Check for redirects
            if status.is_redirection() {
                redirects_made += 1;
                if redirects_made > MAX_REDIRECTS {
                    return Err(ProxyError::TooManyRedirects);
                }

                let location_header = resp
                    .headers()
                    .get(header::LOCATION)
                    .ok_or_else(|| {
                        ProxyError::RemoteServer("Redirect missing Location header".to_string())
                    })?
                    .to_str()
                    .map_err(|_| {
                        ProxyError::RemoteServer("Malformed Location header".to_string())
                    })?;

                let next_url = current_url
                    .join(location_header)
                    .map_err(|e| ProxyError::InvalidUrl(e.to_string()))?;

                // Re-validate SSRF on redirect target
                current_url = ssrf::validate_url(next_url.as_str()).await?;
                continue;
            }

            // Must be 200 OK or 206 Partial Content
            if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
                return Err(ProxyError::UnsupportedStatusCode(status.as_u16()));
            }

            // Validate Content-Type
            let content_type = resp
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_lowercase();

            let matched = allowed_mime_prefixes
                .iter()
                .any(|prefix| content_type.starts_with(prefix));

            if !matched {
                return Err(ProxyError::UnsupportedContentType(if content_type.is_empty() {
                    "unknown".to_string()
                } else {
                    content_type
                }));
            }

            return Ok(resp);
        }
    }
}

pub fn copy_safe_headers(upstream_headers: &reqwest::header::HeaderMap, out_headers: &mut HeaderMap) {
    let safe_headers = [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::CACHE_CONTROL,
        header::ETAG,
        header::LAST_MODIFIED,
    ];

    for name in safe_headers {
        if let Some(val) = upstream_headers.get(&name) {
            if let Ok(clean_val) = HeaderValue::from_bytes(val.as_bytes()) {
                out_headers.insert(name, clean_val);
            }
        }
    }

    // Add CORS headers
    out_headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    out_headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, HEAD, OPTIONS"),
    );
    out_headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("*"),
    );
    out_headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("Content-Length, Content-Range, Accept-Ranges, ETag"),
    );
}
