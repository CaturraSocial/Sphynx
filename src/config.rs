use serde::Deserialize;
use std::{env, fs, path::PathBuf, time::Duration};

#[derive(Debug, Clone, Deserialize)]
pub struct RawConfig {
    #[serde(rename = "PORT")]
    pub port: Option<u16>,
    #[serde(rename = "IP")]
    pub ip: Option<String>,
    #[serde(rename = "CACHE_DIR")]
    pub cache_dir: Option<String>,
    #[serde(rename = "CACHE_MAX_SIZE_MB")]
    pub cache_max_size_mb: Option<u64>,
    #[serde(rename = "CACHE_TTL_HOURS")]
    pub cache_ttl_hours: Option<u64>,
    #[serde(rename = "MAX_IMAGE_SIZE_MB")]
    pub max_image_size_mb: Option<usize>,
    #[serde(rename = "CACHE_MAX_FILE_SIZE_MB")]
    pub cache_max_file_size_mb: Option<usize>,
    #[serde(rename = "CONNECT_TIMEOUT_SECS")]
    pub connect_timeout_secs: Option<u64>,
    #[serde(rename = "READ_TIMEOUT_SECS")]
    pub read_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub ip: String,
    pub port: u16,
    pub cache_dir: PathBuf,
    pub cache_max_size_bytes: u64,
    pub cache_ttl: Duration,
    pub max_image_size_bytes: usize,
    pub cache_max_file_size_bytes: usize,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
}

impl Config {
    pub fn load() -> Self {
        let config_path = env::var("CONFIG_FILE").unwrap_or_else(|_| "config.json".to_string());
        let raw: RawConfig = fs::read_to_string(&config_path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or(RawConfig {
                port: None,
                ip: None,
                cache_dir: None,
                cache_max_size_mb: None,
                cache_ttl_hours: None,
                max_image_size_mb: None,
                cache_max_file_size_mb: None,
                connect_timeout_secs: None,
                read_timeout_secs: None,
            });

        let ip = env::var("IP")
            .ok()
            .or(raw.ip)
            .unwrap_or_else(|| "127.0.0.1".to_string());

        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .or(raw.port)
            .unwrap_or(3000);

        let cache_dir_str = env::var("CACHE_DIR")
            .ok()
            .or(raw.cache_dir)
            .unwrap_or_else(|| "./cache".to_string());
        let cache_dir = PathBuf::from(cache_dir_str);

        let cache_max_size_mb = env::var("CACHE_MAX_SIZE_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(raw.cache_max_size_mb)
            .unwrap_or(1024);
        let cache_max_size_bytes = cache_max_size_mb * 1024 * 1024;

        let cache_ttl_hours = env::var("CACHE_TTL_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(raw.cache_ttl_hours)
            .unwrap_or(24);
        let cache_ttl = Duration::from_secs(cache_ttl_hours * 3600);

        let max_image_size_mb = env::var("MAX_IMAGE_SIZE_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(raw.max_image_size_mb)
            .unwrap_or(20);
        let max_image_size_bytes = max_image_size_mb * 1024 * 1024;

        let cache_max_file_size_mb = env::var("CACHE_MAX_FILE_SIZE_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(raw.cache_max_file_size_mb)
            .unwrap_or(25);
        let cache_max_file_size_bytes = cache_max_file_size_mb * 1024 * 1024;

        let connect_timeout_secs = env::var("CONNECT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(raw.connect_timeout_secs)
            .unwrap_or(10);
        let connect_timeout = Duration::from_secs(connect_timeout_secs);

        let read_timeout_secs = env::var("READ_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or(raw.read_timeout_secs)
            .unwrap_or(30);
        let read_timeout = Duration::from_secs(read_timeout_secs);

        Config {
            ip,
            port,
            cache_dir,
            cache_max_size_bytes,
            cache_ttl,
            max_image_size_bytes,
            cache_max_file_size_bytes,
            connect_timeout,
            read_timeout,
        }
    }
}
