FROM rust:1-bookworm

RUN rustup component add clippy rustfmt \
    && rustup target add x86_64-unknown-linux-musl \
    && apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
