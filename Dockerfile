# ==============================================================================
# Stage 1: Build stage
# ==============================================================================
FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /usr/src/xboard

COPY Cargo.toml Cargo.lock ./

RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs && \
    cargo build --release && \
    rm -rf src

COPY src ./src

RUN touch src/main.rs src/lib.rs && cargo build --release --bin xboard-rs

# ==============================================================================
# Stage 2: Final Minimal Runtime Image
# ==============================================================================
FROM alpine:3.20

RUN apk add --no-cache ca-certificates tzdata sqlite-libs libgcc && \
    addgroup -S -g 1000 xboard && \
    adduser -S -G xboard -u 1000 -h /app xboard && \
    mkdir -p /data /app/public /app/theme && \
    chown -R xboard:xboard /data /app

WORKDIR /app

COPY --from=builder /usr/src/xboard/target/release/xboard-rs /usr/local/bin/xboard-rs

# Pre-copy frontend assets if available in build context
COPY public /app/public
COPY theme /app/theme

ENV SERVER_HOST=0.0.0.0 \
    SERVER_PORT=7001 \
    DB_CONNECTION=sqlite \
    DB_DATABASE=/data/xboard.db \
    APP_NAME=Xboard \
    RUST_LOG=xboard_rs=info,tower_http=info

USER xboard

VOLUME ["/data", "/app/public", "/app/theme"]

EXPOSE 7001

ENTRYPOINT ["xboard-rs"]
