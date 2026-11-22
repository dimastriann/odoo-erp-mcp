# syntax=docker/dockerfile:1

FROM rust:1.88-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home-dir /data --create-home odoo-mcp
COPY --from=builder /build/target/release/odoo-erp-mcp /usr/local/bin/odoo-erp-mcp
WORKDIR /data
ENV ODOO_MCP_CONFIG_PATH=/data/config.json
USER odoo-mcp
EXPOSE 3333 3334
VOLUME ["/data"]
ENTRYPOINT ["odoo-erp-mcp"]
