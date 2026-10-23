# Stage 1: Build
FROM rust:1.80-alpine AS builder

RUN apk add --no-cache musl-dev ca-certificates

WORKDIR /app

# Pre-compile dependencies for Docker layer caching
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "pub fn dummy() {}" > src/lib.rs
RUN cargo build --release
RUN rm -rf src

# Compile application source code
COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release

# Stage 2: Final minimal runtime
FROM alpine:3.20

RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app

# Copy only the compiled binary from the builder stage
COPY --from=builder /app/target/release/sphynx-proxy /app/sphynx-proxy
COPY config.json /app/config.json

# Non-root user and cache directory setup
RUN mkdir -p /app/cache && adduser -D -u 1000 sphynx && chown -R sphynx:sphynx /app

USER sphynx

ENV PORT=3000 \
    IP=0.0.0.0 \
    CACHE_DIR=/app/cache

EXPOSE 3000

CMD ["/app/sphynx-proxy"]
