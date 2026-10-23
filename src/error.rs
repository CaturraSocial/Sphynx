use axum::{
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("Unable to parse URL: {0}")]
    InvalidUrl(String),

    #[error("Unsupported protocol: only http and https are allowed")]
    UnsupportedProtocol,

    #[error("SSRF protection: {0}")]
    SsrfBlocked(String),

    #[error("Unsupported remote server status code {0}")]
    UnsupportedStatusCode(u16),

    #[error("Unsupported content type {0}")]
    UnsupportedContentType(String),

    #[error("Payload too large: {0}")]
    PayloadTooLarge(String),

    #[error("Error contacting remote server: {0}")]
    RemoteServer(String),

    #[error("Too many redirects (max 5)")]
    TooManyRedirects,

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Path {0} not found")]
    NotFound(String),
}

#[derive(Serialize)]
struct JsonErrorResponse<'a> {
    error: &'a str,
    status: u16,
}

impl ProxyError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ProxyError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
            ProxyError::UnsupportedProtocol => StatusCode::BAD_REQUEST,
            ProxyError::SsrfBlocked(_) => StatusCode::BAD_REQUEST,
            ProxyError::UnsupportedStatusCode(code) => {
                if *code == 404 {
                    StatusCode::NOT_FOUND
                } else if *code == 403 {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::BAD_REQUEST
                }
            }
            ProxyError::UnsupportedContentType(_) => StatusCode::BAD_REQUEST,
            ProxyError::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            ProxyError::RemoteServer(_) => StatusCode::BAD_REQUEST,
            ProxyError::TooManyRedirects => StatusCode::BAD_REQUEST,
            ProxyError::NotFound(_) => StatusCode::NOT_FOUND,
            ProxyError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn to_response_with_headers(&self, headers: &HeaderMap) -> Response {
        let status = self.status_code();
        let accepts_json = headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.contains("application/json"))
            .unwrap_or(false);

        if accepts_json {
            let error_msg = self.to_string();
            let body = serde_json::to_string(&JsonErrorResponse {
                error: &error_msg,
                status: status.as_u16(),
            })
            .unwrap_or_else(|_| r#"{"error":"Serialization error","status":500}"#.to_string());

            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(axum::body::Body::from(body))
                .unwrap()
        } else {
            // Legacy Node.js HTML/plain text format
            let body = match self {
                ProxyError::NotFound(_) => "404 Not Found".to_string(),
                ProxyError::Internal(_) => "500 Server Error".to_string(),
                _ => format!("400 Bad Request: {self}"),
            };

            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "text/html")
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(axum::body::Body::from(body))
                .unwrap()
        }
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let default_headers = HeaderMap::new();
        self.to_response_with_headers(&default_headers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn test_legacy_html_error_response() {
        let err = ProxyError::InvalidUrl("http://bad".to_string());
        let headers = HeaderMap::new();
        let resp = err.to_response_with_headers(&headers);

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html"
        );

        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8_lossy(&body_bytes);
        assert!(body_str.starts_with("400 Bad Request: Unable to parse URL: http://bad"));
    }

    #[tokio::test]
    async fn test_json_negotiated_error_response() {
        let err = ProxyError::NotFound("/some/route".to_string());
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        let resp = err.to_response_with_headers(&headers);

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json_val: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json_val["status"], 404);
        assert_eq!(json_val["error"], "Path /some/route not found");
    }
}
