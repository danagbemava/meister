# Rust / Tauri project container — BUILD ONLY
#
# This container handles cargo build, cargo check, cargo test
# The compiled binary/app runs on the HOST — not here
# GPU, camera, Metal, and Tauri window rendering stay on the host
#
# cargo vet is installed here for dependency auditing

FROM devcontainer-base:latest

USER root

# Build dependencies for native crates commonly used in Tauri
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev \
    libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    && rm -rf /var/lib/apt/lists/*

USER dev

# Install rustup as the non-root user
# Pin toolchain version via rust-toolchain.toml in the project — do not rely on "stable" floating
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --no-modify-path --default-toolchain stable

ENV PATH="/home/dev/.cargo/bin:$PATH"

# Install cargo-vet for supply chain auditing
# cargo vet requires explicit auditing of new dependencies before they build
RUN cargo install cargo-vet --locked

# Install cargo-deny for license and advisory checks
RUN cargo install cargo-deny --locked

RUN rustc --version && cargo --version && cargo vet --version
