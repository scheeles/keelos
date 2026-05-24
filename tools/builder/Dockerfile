# NOTE: For production builds, pin this base image by digest, e.g.:
#   FROM debian:bookworm-slim@sha256:<digest>
FROM debian:bookworm-slim

# Detect architecture
ARG TARGETARCH
ARG BUILDPLATFORM

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
    util-linux \
    fdisk \
    qemu-utils \
    xorriso \
    mtools \
    cmake \
    clang \
    iproute2 \
    e2fsprogs \
    libseccomp2 \
    iptables \
    && rm -rf /var/lib/apt/lists/*

# Install GRUB only on x86_64 (not available on ARM)
RUN if [ "$(dpkg --print-architecture)" = "amd64" ]; then \
    apt-get update && apt-get install -y \
    grub-pc-bin \
    grub-efi-amd64-bin \
    && rm -rf /var/lib/apt/lists/*; \
    fi

# --- Checksum variables for downloaded binaries ---
# TODO: Replace each FIXME placeholder with the actual SHA-256 checksum
# obtained from the upstream release page or by running:
#   sha256sum /path/to/downloaded/binary
ARG BUSYBOX_SHA256="FIXME_REPLACE_WITH_ACTUAL_SHA256"
ARG CONTAINERD_SHA256="FIXME_REPLACE_WITH_ACTUAL_SHA256"
ARG KUBELET_SHA256="FIXME_REPLACE_WITH_ACTUAL_SHA256"
ARG RUNC_SHA256="FIXME_REPLACE_WITH_ACTUAL_SHA256"
ARG CRANE_SHA256="FIXME_REPLACE_WITH_ACTUAL_SHA256"
ARG CNI_SHA256="FIXME_REPLACE_WITH_ACTUAL_SHA256"

# Install static busybox (x86_64 for target, will work via qemu)
RUN wget --tries=5 --retry-connrefused --retry-on-http-error=429,500,502,503,504 -O /usr/local/bin/busybox https://busybox.net/downloads/binaries/1.35.0-x86_64-linux-musl/busybox && \
    chmod +x /usr/local/bin/busybox
# Verify checksum
RUN echo "${BUSYBOX_SHA256}  /usr/local/bin/busybox" | sha256sum -c -

# Install static binaries for the runtime (x86_64 target)
RUN export CONTAINERD_VERSION=1.7.11 && \
    wget --tries=5 --retry-connrefused --retry-on-http-error=429,500,502,503,504 -O /tmp/containerd.tar.gz https://github.com/containerd/containerd/releases/download/v${CONTAINERD_VERSION}/containerd-static-${CONTAINERD_VERSION}-linux-amd64.tar.gz && \
    echo "${CONTAINERD_SHA256}  /tmp/containerd.tar.gz" | sha256sum -c - && \
    tar Cxzvf /usr/local /tmp/containerd.tar.gz && \
    rm /tmp/containerd.tar.gz

# Install Kubernetes binaries (x86_64 target)
ARG K8S_VERSION=v1.31.0
RUN wget --tries=5 --retry-connrefused --retry-on-http-error=429,500,502,503,504 -O /usr/local/bin/kubelet https://dl.k8s.io/release/${K8S_VERSION}/bin/linux/amd64/kubelet && \
    chmod +x /usr/local/bin/kubelet
# Verify checksum
RUN echo "${KUBELET_SHA256}  /usr/local/bin/kubelet" | sha256sum -c -

# Install runc (x86_64 target)
RUN export RUNC_VERSION=1.1.10 && \
    wget --tries=5 --retry-connrefused --retry-on-http-error=429,500,502,503,504 -O /tmp/runc.amd64 https://github.com/opencontainers/runc/releases/download/v${RUNC_VERSION}/runc.amd64 && \
    echo "${RUNC_SHA256}  /tmp/runc.amd64" | sha256sum -c - && \
    install -m 755 /tmp/runc.amd64 /usr/local/sbin/runc && \
    rm /tmp/runc.amd64

# Pre-download pause container image for offline use
# This avoids needing network access to pull the image during container creation
RUN CRANE_VERSION=0.19.1 && \
    wget -O /tmp/crane.tar.gz https://github.com/google/go-containerregistry/releases/download/v${CRANE_VERSION}/go-containerregistry_Linux_x86_64.tar.gz && \
    echo "${CRANE_SHA256}  /tmp/crane.tar.gz" | sha256sum -c - && \
    tar -xzf /tmp/crane.tar.gz -C /usr/local/bin crane && \
    rm /tmp/crane.tar.gz && \
    crane pull --platform linux/amd64 registry.k8s.io/pause:3.9 /usr/local/share/pause-3.9.tar && \
    rm /usr/local/bin/crane

# Install CNI plugins (x86_64 target)
RUN export CNI_VERSION=1.6.2 && \
    mkdir -p /opt/cni/bin && \
    wget --tries=5 --retry-connrefused --retry-on-http-error=429,500,502,503,504 -O /tmp/cni-plugins.tgz https://github.com/containernetworking/plugins/releases/download/v${CNI_VERSION}/cni-plugins-linux-amd64-v${CNI_VERSION}.tgz && \
    echo "${CNI_SHA256}  /tmp/cni-plugins.tgz" | sha256sum -c - && \
    tar Cxzvf /opt/cni/bin /tmp/cni-plugins.tgz && \
    rm /tmp/cni-plugins.tgz

# Install Rust (stable)
# NOTE: For production builds, pin the toolchain version explicitly, e.g.:
#   sh -s -- -y --default-toolchain 1.87.0
# The workspace rust-toolchain.toml controls the version used during compilation.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install musl target for cross-compilation
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /keelos
