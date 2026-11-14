# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY rust/ rust/
COPY rust-server/ rust-server/
RUN cargo build --manifest-path rust-server/Cargo.toml --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends age ca-certificates tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd -g 10001 multi-backup && useradd -u 10001 -g 10001 -M -s /usr/sbin/nologin multi-backup \
    && mkdir -p /data && chown 10001:10001 /data
COPY --from=builder /build/rust-server/target/release/multi-backup-server /usr/local/bin/multi-backup-server
USER 10001:10001
EXPOSE 8080
VOLUME ["/data"]
ENV MULTI_BACKUP_BIND=0.0.0.0:8080 MULTI_BACKUP_DATA_ROOT=/data/backups MULTI_BACKUP_EXPORT_ROOT=/data/exports
ENTRYPOINT ["/usr/bin/tini","--","/usr/local/bin/multi-backup-server"]
