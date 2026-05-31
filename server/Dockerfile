FROM rust:1.87-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY protocol/ protocol/
COPY server/ server/

RUN cargo build -p server --release && strip target/release/server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/server /usr/local/bin/server

EXPOSE 9880

CMD ["server", "--config", "/etc/omniproxy/config.yml"]
