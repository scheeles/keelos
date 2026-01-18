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

# Install static binaries for the runtime (x86_64)
RUN export CONTAINERD_VERSION=1.7.11 && \
    wget https://github.com/containerd/containerd/releases/download/v${CONTAINERD_VERSION}/containerd-static-${CONTAINERD_VERSION}-linux-amd64.tar.gz && \
    tar Cxzvf /usr/local /containerd-static-${CONTAINERD_VERSION}-linux-amd64.tar.gz && \
    rm /containerd-static-${CONTAINERD_VERSION}-linux-amd64.tar.gz

# Install Kubernetes binaries
ARG K8S_VERSION=v1.29.0
RUN wget -O /usr/local/bin/kubelet https://dl.k8s.io/release/${K8S_VERSION}/bin/linux/amd64/kubelet && \
    chmod +x /usr/local/bin/kubelet

RUN export RUNC_VERSION=1.1.10 && \
    wget https://github.com/opencontainers/runc/releases/download/v${RUNC_VERSION}/runc.amd64 && \
    install -m 755 runc.amd64 /usr/local/sbin/runc && \
    rm runc.amd64

RUN export CNI_VERSION=1.3.0 && \
    mkdir -p /opt/cni/bin && \
    wget https://github.com/containernetworking/plugins/releases/download/v${CNI_VERSION}/cni-plugins-linux-amd64-v${CNI_VERSION}.tgz && \
    tar Cxzvf /opt/cni/bin /cni-plugins-linux-amd64-v${CNI_VERSION}.tgz && \
    rm /cni-plugins-linux-amd64-v${CNI_VERSION}.tgz

# Install Rust (stable)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install musl target
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /maticos
