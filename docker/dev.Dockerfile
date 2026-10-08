FROM rust:1-bookworm

RUN rustup component add clippy rustfmt llvm-tools-preview \
    && rustup target add x86_64-unknown-linux-musl \
    && apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*

# Prebuilt binaries from the projects' own releases; cargo-mutants publishes none
# for arm64, so on Apple Silicon it is compiled from crates.io instead.
RUN curl -fsSL "https://github.com/cargo-bins/cargo-binstall/releases/download/v1.25.2/cargo-binstall-$(uname -m)-unknown-linux-musl.tgz" \
        | tar -xz -C "$CARGO_HOME/bin" \
    && cargo binstall --no-confirm --locked --disable-strategies quick-install \
        cargo-llvm-cov@0.9.1 cargo-mutants@27.1.0 cargo-insta@1.49.0 \
    && rm -rf "$CARGO_HOME/registry"

WORKDIR /src
