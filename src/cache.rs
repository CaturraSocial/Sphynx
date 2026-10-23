use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::fs;
use tracing::{debug, error, info, warn};

use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMeta {
    pub content_type: String,
    pub created_at: u64,
    pub etag: String,
    pub size: u64,
}

pub fn parse_cache_meta(data: &[u8]) -> Option<CacheMeta> {
    if let Ok(meta) = serde_json::from_slice::<CacheMeta>(data) {
        return Some(meta);
    }
    // If standard JSON parsing fails, sanitize leading zeros in numbers (e.g. :0792761714 -> :792761714)
    let s = std::str::from_utf8(data).ok()?;
    let mut cleaned = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        cleaned.push(c);
        if c == ':' {
            while let Some(&next) = chars.peek() {
                if next.is_whitespace() {
                    cleaned.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            let mut zeros_count = 0;
            while let Some(&next) = chars.peek() {
                if next == '0' {
                    zeros_count += 1;
                    chars.next();
                } else {
                    break;
                }
            }
            if zeros_count > 0 {
                if let Some(&next) = chars.peek() {
                    if next.is_ascii_digit() && next != '0' {
                        // Stripped leading zeros
                    } else {
                        cleaned.push('0');
                    }
                } else {
                    cleaned.push('0');
                }
            }
        }
    }
    serde_json::from_str::<CacheMeta>(&cleaned).ok()
}

#[derive(Clone)]
pub struct DiskCache {
    cache_dir: PathBuf,
    max_size_bytes: u64,
    ttl_secs: u64,
}

impl DiskCache {
    pub fn new(config: &Config) -> Self {
        Self {
            cache_dir: config.cache_dir.clone(),
            max_size_bytes: config.cache_max_size_bytes,
            ttl_secs: config.cache_ttl.as_secs(),
        }
    }

    pub fn generate_key(
        url: &str,
        format: &str,
        width: Option<u32>,
        height: Option<u32>,
        quality: Option<u8>,
    ) -> String {
        let mut hasher = Sha256::new();
        let canonical = format!(
            "{}|{}|{}|{}|{}",
            url,
            format,
            width.unwrap_or(0),
            height.unwrap_or(0),
            quality.unwrap_or(0)
        );
        hasher.update(canonical.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub async fn init(&self) -> std::io::Result<()> {
        if !self.cache_dir.exists() {
            fs::create_dir_all(&self.cache_dir).await?;
        }
        Ok(())
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    pub async fn get(&self, key: &str) -> Option<(CacheMeta, fs::File)> {
        let meta_path = self.cache_dir.join(format!("{key}.meta"));
        let bin_path = self.cache_dir.join(format!("{key}.bin"));

        let meta_data = fs::read(&meta_path).await.ok()?;
        let meta = match parse_cache_meta(&meta_data) {
            Some(m) => m,
            None => {
                warn!("Corrupted cache metadata for {key}. Removing.");
                let _ = fs::remove_file(&meta_path).await;
                let _ = fs::remove_file(&bin_path).await;
                return None;
            }
        };

        let now = Self::now_secs();
        if now.saturating_sub(meta.created_at) >= self.ttl_secs {
            debug!("Cache entry for key {key} expired. Removing.");
            let _ = fs::remove_file(&meta_path).await;
            let _ = fs::remove_file(&bin_path).await;
            return None;
        }

        let file = fs::File::open(&bin_path).await.ok()?;
        Some((meta, file))
    }

    pub async fn put(
        &self,
        key: &str,
        content_type: &str,
        data: &[u8],
    ) -> std::io::Result<CacheMeta> {
        self.init().await?;

        let etag = format!("\"sphynx-{key}\"");
        let now = Self::now_secs();
        let meta = CacheMeta {
            content_type: content_type.to_string(),
            created_at: now,
            etag: etag.clone(),
            size: data.len() as u64,
        };

        let temp_bin_path = self.cache_dir.join(format!("{key}.tmp.bin"));
        let bin_path = self.cache_dir.join(format!("{key}.bin"));
        let meta_path = self.cache_dir.join(format!("{key}.meta"));

        fs::write(&temp_bin_path, data).await?;
        fs::rename(&temp_bin_path, &bin_path).await?;

        let meta_bytes = serde_json::to_vec(&meta).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?;
        fs::write(&meta_path, &meta_bytes).await?;

        debug!("Stored cached item {key} ({} bytes)", data.len());
        Ok(meta)
    }

    pub async fn prune(&self) {
        if !self.cache_dir.exists() {
            return;
        }

        let mut read_dir = match fs::read_dir(&self.cache_dir).await {
            Ok(rd) => rd,
            Err(e) => {
                error!("Failed to read cache directory for pruning: {e}");
                return;
            }
        };

        let now = Self::now_secs();
        let mut total_size: u64 = 0;
        let mut entries: Vec<(PathBuf, u64, u64)> = Vec::new(); // (path, size, timestamp)

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "meta" {
                    let key = path.file_stem().unwrap_or_default().to_string_lossy();
                    let bin_path = self.cache_dir.join(format!("{key}.bin"));

                    if let Ok(data) = fs::read(&path).await {
                        match parse_cache_meta(&data) {
                            Some(meta) => {
                                if now.saturating_sub(meta.created_at) >= self.ttl_secs {
                                    let _ = fs::remove_file(&path).await;
                                    let _ = fs::remove_file(&bin_path).await;
                                    debug!("Pruned expired cache item: {key}");
                                    continue;
                                }
                            }
                            None => {
                                // Corrupted or invalid JSON meta -> remove both
                                let _ = fs::remove_file(&path).await;
                                let _ = fs::remove_file(&bin_path).await;
                                debug!("Pruned invalid metadata cache item: {key}");
                                continue;
                            }
                        }
                    }
                } else if ext == "bin" {
                    let key = path.file_stem().unwrap_or_default().to_string_lossy();
                    let meta_path = self.cache_dir.join(format!("{key}.meta"));

                    // If .bin has no .meta, it's an orphaned data file
                    if !meta_path.exists() {
                        let _ = fs::remove_file(&path).await;
                        continue;
                    }

                    if let Ok(meta) = entry.metadata().await {
                        let size = meta.len();
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        total_size += size;
                        entries.push((path, size, modified));
                    }
                }
            }
        }

        // Check if cache size exceeds max_size_bytes
        if total_size > self.max_size_bytes {
            let target_size = (self.max_size_bytes as f64 * 0.8) as u64; // Reduce to 80%
            info!(
                "Cache size ({} bytes) exceeds limit ({} bytes). Pruning to {} bytes.",
                total_size, self.max_size_bytes, target_size
            );

            // Sort by modified time ascending (oldest first)
            entries.sort_by_key(|(_, _, modified)| *modified);

            for (path, size, _) in entries {
                if total_size <= target_size {
                    break;
                }
                let key = path.file_stem().unwrap_or_default().to_string_lossy();
                let meta_path = self.cache_dir.join(format!("{key}.meta"));
                let _ = fs::remove_file(&path).await;
                let _ = fs::remove_file(&meta_path).await;
                total_size = total_size.saturating_sub(size);
                debug!("Pruned LRU cache item: {key} ({size} bytes)");
            }
        }
    }

    pub fn start_pruning_task(self: Arc<Self>) {
        let cache = self.clone();
        tokio::spawn(async move {
            // Prune immediately on startup
            cache.prune().await;

            let mut interval = tokio::time::interval(Duration::from_secs(600)); // Every 10 minutes
            interval.tick().await; // Consume initial immediate tick
            loop {
                interval.tick().await;
                cache.prune().await;
            }
        });
    }

    pub async fn stats(&self) -> (u64, usize) {
        if !self.cache_dir.exists() {
            return (0, 0);
        }
        let mut read_dir = match fs::read_dir(&self.cache_dir).await {
            Ok(rd) => rd,
            Err(_) => return (0, 0),
        };
        let mut total_size = 0u64;
        let mut count = 0usize;
        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "bin" {
                    if let Ok(meta) = entry.metadata().await {
                        total_size += meta.len();
                        count += 1;
                    }
                }
            }
        }
        (total_size, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_put_get_and_expiration() {
        let temp_dir = std::env::temp_dir().join(format!("sphynx_cache_test_{}", std::time::SystemTime::now().elapsed().unwrap().as_nanos()));
        let mut config = Config::load();
        config.cache_dir = temp_dir.clone();
        config.cache_ttl = Duration::from_secs(2); // 2 second TTL
        config.cache_max_size_bytes = 100_000;

        let cache = DiskCache::new(&config);
        cache.init().await.unwrap();

        let key = DiskCache::generate_key("http://example.com/img.png", "image/webp", Some(100), Some(100), Some(80));
        let data = b"fake-webp-image-data-payload";

        // Put into cache
        let meta = cache.put(&key, "image/webp", data).await.unwrap();
        assert_eq!(meta.content_type, "image/webp");
        assert_eq!(meta.size, data.len() as u64);

        // Get from cache (immediate hit)
        let hit = cache.get(&key).await;
        assert!(hit.is_some());
        let (retrieved_meta, _) = hit.unwrap();
        assert_eq!(retrieved_meta.etag, meta.etag);

        // Wait for TTL expiration (crossing the 2-second boundary)
        tokio::time::sleep(Duration::from_millis(2100)).await;

        // Get should now return None and remove expired files
        let expired = cache.get(&key).await;
        assert!(expired.is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_cache_max_size_pruning() {
        let temp_dir = std::env::temp_dir().join(format!("sphynx_cache_prune_test_{}", DiskCache::now_secs()));
        let mut config = Config::load();
        config.cache_dir = temp_dir.clone();
        config.cache_ttl = Duration::from_secs(3600);
        config.cache_max_size_bytes = 500; // 500 bytes max

        let cache = DiskCache::new(&config);
        cache.init().await.unwrap();

        // Write 3 items of 200 bytes each = 600 bytes (exceeds 500)
        let chunk = vec![1u8; 200];
        cache.put("item1", "image/png", &chunk).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        cache.put("item2", "image/png", &chunk).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        cache.put("item3", "image/png", &chunk).await.unwrap();

        // Trigger prune
        cache.prune().await;

        // Item 1 (oldest) should have been pruned to bring total size under 80% (400 bytes)
        assert!(cache.get("item1").await.is_none());
        // Newer items should remain
        assert!(cache.get("item3").await.is_some());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[test]
    fn test_parse_cache_meta_with_leading_zeros() {
        let raw = br#"{"content_type":"image/webp","created_at":0792761714,"etag":"\"sphynx-abc\"","size":65374}"#;
        let meta = parse_cache_meta(raw);
        assert!(meta.is_some());
        let meta = meta.unwrap();
        assert_eq!(meta.created_at, 792761714);
        assert_eq!(meta.content_type, "image/webp");
    }
}
