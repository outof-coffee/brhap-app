# Build environment for the Linux AppImage. Not a multi-stage build: the
# repo is bind-mounted in at `run` time (see release-linux.sh), so this
# image only needs the toolchain, not a copy of the source.
FROM rust:1.88-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    cmake \
    libwayland-client0 \
    libwayland-egl1 \
    libxkbcommon0 \
    libx11-6 \
    libx11-xcb1 \
    libxcb1 \
    libvulkan1 \
    libfuse2 \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-packager --locked --version 0.11.8

WORKDIR /work
