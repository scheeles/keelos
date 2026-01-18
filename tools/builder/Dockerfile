FROM debian:bookworm-slim

# Install core build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    bc \
    bison \
    flex \
    libelf-dev \
    libssl-dev \
    cpio \
    git \
    curl \
    wget \
    unzip \
    rsync \
    qemu-system-x86 \
    gcc-x86-64-linux-gnu \
    musl-tools \
    protobuf-compiler \
    # Cross-compilation deps if needed, though we build on Linux for Linux usually
    # But host might be mac, so this container is Linux x86_64
    && rm -rf /var/lib/apt/lists/*

# Install Rust (stable)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install musl target
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /maticos
