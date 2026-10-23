# Sphynx

Media proxy server for images, audio, and video.

## Quick Usage

Pass the target file URL via the `?url=` parameter:

- **Images:** `/image?url=<target>` or `/v1/image?url=<target>`
  - Optional query parameters:
    - `&w=800` (width in px, keeps aspect ratio)
    - `&h=600` (height in px, keeps aspect ratio)
    - `&format=webp` (`webp`, `jpeg`, `png`, or `original`)
    - `&q=80` (quality, 1–100)
- **Audio:** `/audio?url=<target>` or `/v1/audio?url=<target>`
- **Video:** `/video?url=<target>` or `/v1/video?url=<target>`
- **Any media:** `/?url=<target>` or `/v1/?url=<target>`
- **Health check:** `/health` or `/v1/health`

---

## Self-Hosting

### Building & Running

Requirements: [Rust](https://rustup.rs/) (cargo) or [Docker](https://www.docker.com/)

**Using Cargo:**
```bash
# Build & run
cargo build --release
./target/release/sphynx-proxy
```

**Using Docker:**
```bash
docker build -t sphynx-proxy .
docker run -d -p 3000:3000 -v ./cache:/app/cache sphynx-proxy
```

### Configuration

Sphynx reads from `config.json` by default. Any value can be overridden with an environment variable.

#### `config.json`
```json
{
  "PORT": 3000,
  "IP": "127.0.0.1",
  "CACHE_DIR": "./cache",
  "CACHE_MAX_SIZE_MB": 1024,
  "CACHE_TTL_HOURS": 24,
  "MAX_IMAGE_SIZE_MB": 20,
  "CACHE_MAX_FILE_SIZE_MB": 25,
  "CONNECT_TIMEOUT_SECS": 10,
  "READ_TIMEOUT_SECS": 30
}
```

#### Settings Reference

| Key / Env Var | Default | Description |
|---|---|---|
| `CONFIG_FILE` | `config.json` | Path to JSON config file |
| `PORT` | `3000` | Port to listen on |
| `IP` | `127.0.0.1` | IP address to bind |
| `CACHE_DIR` | `./cache` | Directory where converted images are cached |
| `CACHE_MAX_SIZE_MB` | `1024` | Cache disk limit in MB (oldest files pruned when full) |
| `CACHE_TTL_HOURS` | `24` | Cache expiration time in hours |
| `MAX_IMAGE_SIZE_MB` | `20` | Max upstream file size buffered for image processing |
| `CACHE_MAX_FILE_SIZE_MB` | `25` | Max file size cached to disk (larger files stream direct) |
| `CONNECT_TIMEOUT_SECS` | `10` | Upstream connection timeout in seconds |
| `READ_TIMEOUT_SECS` | `30` | Upstream read/streaming timeout in seconds |