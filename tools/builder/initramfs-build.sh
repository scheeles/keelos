#!/bin/bash
set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Variant-aware target directory: build-variant.sh sets VARIANT_TARGET_PROFILE_DIR
if [ -n "${VARIANT_TARGET_PROFILE_DIR:-}" ]; then
    TARGET_DIR="${VARIANT_TARGET_PROFILE_DIR}"
else
    TARGET_DIR="${PROJECT_ROOT}/target/x86_64-unknown-linux-musl/release"
fi

OUTPUT_DIR="${PROJECT_ROOT}/build"
INITRAMFS_DIR="${OUTPUT_DIR}/initramfs"

rm -rf "${INITRAMFS_DIR}"
mkdir -p "${INITRAMFS_DIR}"

mkdir -p "${INITRAMFS_DIR}/bin"
mkdir -p "${INITRAMFS_DIR}/sbin"
mkdir -p "${INITRAMFS_DIR}/etc"
mkdir -p "${INITRAMFS_DIR}/proc"
mkdir -p "${INITRAMFS_DIR}/sys"
mkdir -p "${INITRAMFS_DIR}/usr/bin"
mkdir -p "${INITRAMFS_DIR}/usr/sbin"
mkdir -p "${INITRAMFS_DIR}/opt/cni/bin"
mkdir -p "${INITRAMFS_DIR}/etc/cni/net.d"
mkdir -p "${INITRAMFS_DIR}/var/lib/containerd"
mkdir -p "${INITRAMFS_DIR}/run/containerd"
mkdir -p "${INITRAMFS_DIR}/etc/containerd"
mkdir -p "${INITRAMFS_DIR}/lib/modules"
mkdir -p "${OUTPUT_DIR}"

echo ">>> Copying external binaries..."
# These come from the build container's /usr/local/bin or /usr/local/sbin
cp -L /usr/local/bin/containerd* "${INITRAMFS_DIR}/usr/bin/"
cp -L /usr/local/bin/ctr "${INITRAMFS_DIR}/usr/bin/"
cp -L /usr/local/sbin/runc "${INITRAMFS_DIR}/usr/sbin/"
cp -rL /opt/cni/bin/* "${INITRAMFS_DIR}/opt/cni/bin/"

# Copy iptables (required by kubelet and kube-proxy for network rules)
echo ">>> Copying iptables..."
for bin in iptables iptables-save iptables-restore ip6tables ip6tables-save ip6tables-restore xtables-legacy-multi xtables-nft-multi; do
    for dir in /usr/sbin /sbin /usr/bin /bin; do
        if [ -f "${dir}/${bin}" ]; then
            cp -L "${dir}/${bin}" "${INITRAMFS_DIR}/usr/sbin/"
            break
        fi
    done
done
# Some systems use alternatives - ensure the main iptables binary exists
if [ ! -f "${INITRAMFS_DIR}/usr/sbin/iptables" ]; then
    # Try iptables-legacy or iptables-nft
    if [ -f "/usr/sbin/iptables-legacy" ]; then
        cp -L /usr/sbin/iptables-legacy "${INITRAMFS_DIR}/usr/sbin/iptables"
        cp -L /usr/sbin/ip6tables-legacy "${INITRAMFS_DIR}/usr/sbin/ip6tables" 2>/dev/null || true
    elif [ -f "${INITRAMFS_DIR}/usr/sbin/xtables-nft-multi" ]; then
        ln -sf xtables-nft-multi "${INITRAMFS_DIR}/usr/sbin/iptables"
        ln -sf xtables-nft-multi "${INITRAMFS_DIR}/usr/sbin/ip6tables"
    fi
fi
cp -L "${PROJECT_ROOT}/tools/builder/containerd-config.toml" "${INITRAMFS_DIR}/etc/containerd/config.toml"

# Copy pre-downloaded container images for offline use
echo ">>> Copying pre-downloaded container images..."
mkdir -p "${INITRAMFS_DIR}/usr/share/keel/images"
if [ -f /usr/local/share/pause-3.9.tar ]; then
    cp -L /usr/local/share/pause-3.9.tar "${INITRAMFS_DIR}/usr/share/keel/images/"
    echo "  Copied pause:3.9 image"
else
    echo "  WARNING: pause-3.9.tar not found"
fi

echo ">>> Copying GLIBC libraries (for dynamic Kubelet)..."
mkdir -p "${INITRAMFS_DIR}/lib64"
mkdir -p "${INITRAMFS_DIR}/lib"

# Determine the GLIBC library path - support both cross-compiler and native paths
if [ -d "/usr/x86_64-linux-gnu/lib" ]; then
    GLIBC_LIB_PATH="/usr/x86_64-linux-gnu/lib"
elif [ -d "/lib/x86_64-linux-gnu" ]; then
    GLIBC_LIB_PATH="/lib/x86_64-linux-gnu"
else
    echo "ERROR: Could not find GLIBC library path"
    exit 1
fi
echo "Using GLIBC libraries from: ${GLIBC_LIB_PATH}"

# Interpreter (Must match Requesting program interpreter path)
cp -L "${GLIBC_LIB_PATH}/ld-linux-x86-64.so.2" "${INITRAMFS_DIR}/lib64/"
# Dependencies
cp -L "${GLIBC_LIB_PATH}/libc.so.6" "${INITRAMFS_DIR}/lib/"
# libpthread is often merged into libc in newer glibc versions
if [ -f "${GLIBC_LIB_PATH}/libpthread.so.0" ]; then cp -L "${GLIBC_LIB_PATH}/libpthread.so.0" "${INITRAMFS_DIR}/lib/"; fi
if [ -f "${GLIBC_LIB_PATH}/libresolv.so.2" ]; then cp -L "${GLIBC_LIB_PATH}/libresolv.so.2" "${INITRAMFS_DIR}/lib/"; fi
# Optional but recommended
if [ -f "${GLIBC_LIB_PATH}/libdl.so.2" ]; then cp -L "${GLIBC_LIB_PATH}/libdl.so.2" "${INITRAMFS_DIR}/lib/"; fi
if [ -f "${GLIBC_LIB_PATH}/libm.so.6" ]; then cp -L "${GLIBC_LIB_PATH}/libm.so.6" "${INITRAMFS_DIR}/lib/"; fi
# libseccomp - required by runc for container security
if [ -f "${GLIBC_LIB_PATH}/libseccomp.so.2" ]; then
    cp -L "${GLIBC_LIB_PATH}/libseccomp.so.2" "${INITRAMFS_DIR}/lib/"
elif [ -f "/usr/lib/x86_64-linux-gnu/libseccomp.so.2" ]; then
    cp -L "/usr/lib/x86_64-linux-gnu/libseccomp.so.2" "${INITRAMFS_DIR}/lib/"
else
    echo "WARNING: libseccomp.so.2 not found - runc may fail"
fi
# iptables libraries - required by iptables/ip6tables
for lib in libip4tc libip6tc libxtables libnftnl libmnl; do
    for pattern in "${GLIBC_LIB_PATH}/${lib}.so"* "/usr/lib/x86_64-linux-gnu/${lib}.so"*; do
        if [ -f "${pattern}" ]; then
            cp -L "${pattern}" "${INITRAMFS_DIR}/lib/" 2>/dev/null || true
        fi
    done
done

cp -L "${PROJECT_ROOT}/tools/builder/containerd-config.toml" "${INITRAMFS_DIR}/etc/containerd/config.toml"

echo ">>> Copying CA certificates (for TLS verification)..."
mkdir -p "${INITRAMFS_DIR}/etc/ssl/certs"
if [ -f /etc/ssl/certs/ca-certificates.crt ]; then
    cp -L /etc/ssl/certs/ca-certificates.crt "${INITRAMFS_DIR}/etc/ssl/certs/"
    echo "    Copied CA certificates bundle"
else
    echo "    WARNING: No CA certificates found in build container"
fi

echo ">>> Copying Kubernetes binaries..."
mkdir -p "${INITRAMFS_DIR}/var/lib/kubelet"
mkdir -p "${INITRAMFS_DIR}/var/lib/kubelet/pki"  # For TLS certificates generated via bootstrap
mkdir -p "${INITRAMFS_DIR}/etc/kubernetes/manifests"
cp -L /usr/local/bin/kubelet "${INITRAMFS_DIR}/usr/bin/"
cp -L "${PROJECT_ROOT}/tools/builder/kubelet-config.yaml" "${INITRAMFS_DIR}/etc/kubernetes/kubelet-config.yaml"

echo ">>> Building keel-init..."
# In a real scenario, this runs inside the docker container
echo "Running cargo build..."
if [ "${VARIANT_CARGO_PROFILE:-release}" = "dev" ]; then
    cargo build --target x86_64-unknown-linux-musl --package keel-init --package keel-agent --package osctl
else
    cargo build --release --target x86_64-unknown-linux-musl --package keel-init --package keel-agent --package osctl
fi

# Check if binary exists (assuming user ran build or we are mocking)
if [ ! -f "${TARGET_DIR}/keel-init" ]; then
    echo "ERROR: keel-init binary not found at ${TARGET_DIR}/keel-init"
    echo "Please run: cargo build --release --target x86_64-unknown-linux-musl"
    exit 1
else
    echo "Copying keel-init to /init..."
    cp "${TARGET_DIR}/keel-init" "${INITRAMFS_DIR}/init"

    if [ -f "${TARGET_DIR}/keel-agent" ]; then
        echo "Copying keel-agent to /usr/bin/keel-agent..."
        cp "${TARGET_DIR}/keel-agent" "${INITRAMFS_DIR}/usr/bin/keel-agent"
    fi

    if [ -f "${TARGET_DIR}/osctl" ]; then
        echo "Copying osctl to /usr/bin/osctl..."
        cp "${TARGET_DIR}/osctl" "${INITRAMFS_DIR}/usr/bin/osctl"
    fi
fi

echo ">>> Copying Busybox..."
cp /usr/local/bin/busybox "${INITRAMFS_DIR}/bin/busybox"
ln -sf busybox "${INITRAMFS_DIR}/bin/sh"
ln -sf busybox "${INITRAMFS_DIR}/bin/ifconfig"
ln -sf busybox "${INITRAMFS_DIR}/bin/route"
ln -sf busybox "${INITRAMFS_DIR}/bin/hostname"
ln -sf ../bin/busybox "${INITRAMFS_DIR}/bin/mount"
ln -sf ../bin/busybox "${INITRAMFS_DIR}/bin/umount"
ln -sf ../bin/busybox "${INITRAMFS_DIR}/sbin/mount"
ln -sf ../bin/busybox "${INITRAMFS_DIR}/sbin/umount"
# Kubelet looks for mount in /usr/bin and standard PATH locations
mkdir -p "${INITRAMFS_DIR}/usr/bin"
ln -sf ../../bin/busybox "${INITRAMFS_DIR}/usr/bin/mount"
ln -sf ../../bin/busybox "${INITRAMFS_DIR}/usr/bin/umount"
# Filesystem tools for persistent storage auto-formatting (real e2fsprogs binary)
echo ">>> Copying mkfs.ext4 from e2fsprogs..."
if [ -f /sbin/mkfs.ext4 ]; then
    cp -L /sbin/mkfs.ext4 "${INITRAMFS_DIR}/sbin/mkfs.ext4"
elif [ -f /usr/sbin/mkfs.ext4 ]; then
    cp -L /usr/sbin/mkfs.ext4 "${INITRAMFS_DIR}/sbin/mkfs.ext4"
else
    echo "WARNING: mkfs.ext4 not found in builder"
fi
# Copy e2fsprogs shared library dependencies
for lib in libext2fs.so.2 libcom_err.so.2 libe2p.so.2 libblkid.so.1 libuuid.so.1; do
    find /lib /usr/lib -name "$lib" -exec cp -L {} "${INITRAMFS_DIR}/lib/" \; 2>/dev/null || true
done

echo ">>> Copying iproute2 (for advanced networking)..."
# iproute2 provides full-featured ip command for VLAN, bonding, etc.
# However, copying the host binary often fails due to missing shared libraries.
# Use BusyBox ip by default for reliability in this minimal environment.
echo "Using BusyBox ip command"
ln -sf ../bin/busybox "${INITRAMFS_DIR}/sbin/ip"

echo ">>> Copying kernel modules for networking..."
# Copy VLAN (802.1Q) and bonding kernel modules if available
KERNEL_VERSION=$(uname -r)
MODULE_DIR="/lib/modules/${KERNEL_VERSION}/kernel/drivers/net"

if [ -d "${MODULE_DIR}" ]; then
    # VLAN support (8021q module)
    if [ -f "${MODULE_DIR}/8021q.ko" ] || [ -f "${MODULE_DIR}/8021q.ko.xz" ]; then
        mkdir -p "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}/kernel/drivers/net"
        cp -L "${MODULE_DIR}"/8021q.ko* "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}/kernel/drivers/net/" 2>/dev/null || true
        echo "Copied VLAN (8021q) kernel module"
    fi
    
    # Bonding support
    if [ -f "${MODULE_DIR}/bonding/bonding.ko" ] || [ -f "${MODULE_DIR}/bonding/bonding.ko.xz" ]; then
        mkdir -p "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}/kernel/drivers/net/bonding"
        cp -L "${MODULE_DIR}/bonding"/bonding.ko* "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}/kernel/drivers/net/bonding/" 2>/dev/null || true
        echo "Copied bonding kernel module"
    fi
    
    # Create modules.dep if modules were copied
    if [ -d "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}" ]; then
        echo "# Minimal modules.dep for initramfs" > "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}/modules.dep"
        echo "kernel/drivers/net/8021q.ko:" >> "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}/modules.dep" 2>/dev/null || true
        echo "kernel/drivers/net/bonding/bonding.ko:" >> "${INITRAMFS_DIR}/lib/modules/${KERNEL_VERSION}/modules.dep" 2>/dev/null || true
    fi
else
    echo "WARNING: Kernel modules directory not found at ${MODULE_DIR}"
    echo "VLAN and bonding support may not be available"
fi

# =============================================================================
# Variant-specific content
# =============================================================================
# These sections are controlled by environment variables set by build-variant.sh.

# Cloud metadata agent (for cloud variants)
if [ "${VARIANT_CLOUD_INIT:-false}" = "true" ]; then
    echo ">>> Adding cloud metadata agent..."
    mkdir -p "${INITRAMFS_DIR}/etc/keel"

    # Install cloud-init metadata fetch script
    # This lightweight script queries IMDS (Instance Metadata Service) endpoints
    # at boot to configure hostname, network, and SSH keys from cloud providers.
    cat > "${INITRAMFS_DIR}/usr/bin/keel-cloud-init" << 'CLOUD_EOF'
#!/bin/sh
# KeelOS Cloud Metadata Agent
# Fetches instance metadata from cloud provider IMDS endpoints at boot.
# Supports AWS, GCP, and Azure metadata services.

set -e

METADATA_DIR="/run/keel/metadata"
mkdir -p "${METADATA_DIR}"

log() { echo "[cloud-init] $*"; }

# Detect cloud provider by probing metadata endpoints
detect_provider() {
    # AWS IMDSv2
    if TOKEN=$(wget -q -O - --header "X-aws-ec2-metadata-token-ttl-seconds: 60" \
        --method PUT "http://169.254.169.254/latest/api/token" 2>/dev/null); then
        echo "aws"
        echo "${TOKEN}" > "${METADATA_DIR}/imds-token"
        return
    fi

    # GCP
    if wget -q -O /dev/null --header "Metadata-Flavor: Google" \
        "http://metadata.google.internal/computeMetadata/v1/" 2>/dev/null; then
        echo "gcp"
        return
    fi

    # Azure
    if wget -q -O /dev/null --header "Metadata: true" \
        "http://169.254.169.254/metadata/instance?api-version=2021-02-01" 2>/dev/null; then
        echo "azure"
        return
    fi

    echo "unknown"
}

fetch_aws() {
    TOKEN=$(cat "${METADATA_DIR}/imds-token" 2>/dev/null || true)
    HDR="X-aws-ec2-metadata-token: ${TOKEN}"
    BASE="http://169.254.169.254/latest/meta-data"

    HOSTNAME=$(wget -q -O - --header "${HDR}" "${BASE}/hostname" 2>/dev/null || true)
    INSTANCE_ID=$(wget -q -O - --header "${HDR}" "${BASE}/instance-id" 2>/dev/null || true)
    REGION=$(wget -q -O - --header "${HDR}" "${BASE}/placement/region" 2>/dev/null || true)

    echo "${HOSTNAME}" > "${METADATA_DIR}/hostname"
    echo "${INSTANCE_ID}" > "${METADATA_DIR}/instance-id"
    echo "${REGION}" > "${METADATA_DIR}/region"
    echo "aws" > "${METADATA_DIR}/provider"
}

fetch_gcp() {
    HDR="Metadata-Flavor: Google"
    BASE="http://metadata.google.internal/computeMetadata/v1"

    HOSTNAME=$(wget -q -O - --header "${HDR}" "${BASE}/instance/hostname" 2>/dev/null || true)
    INSTANCE_ID=$(wget -q -O - --header "${HDR}" "${BASE}/instance/id" 2>/dev/null || true)
    ZONE=$(wget -q -O - --header "${HDR}" "${BASE}/instance/zone" 2>/dev/null || true)

    echo "${HOSTNAME}" > "${METADATA_DIR}/hostname"
    echo "${INSTANCE_ID}" > "${METADATA_DIR}/instance-id"
    echo "${ZONE}" > "${METADATA_DIR}/zone"
    echo "gcp" > "${METADATA_DIR}/provider"
}

fetch_azure() {
    HDR="Metadata: true"
    BASE="http://169.254.169.254/metadata/instance?api-version=2021-02-01"

    META=$(wget -q -O - --header "${HDR}" "${BASE}" 2>/dev/null || true)
    # Azure returns JSON; extract fields with simple pattern matching
    HOSTNAME=$(echo "${META}" | sed -n 's/.*"name":"\([^"]*\)".*/\1/p' | head -1)
    VMID=$(echo "${META}" | sed -n 's/.*"vmId":"\([^"]*\)".*/\1/p' | head -1)
    LOCATION=$(echo "${META}" | sed -n 's/.*"location":"\([^"]*\)".*/\1/p' | head -1)

    echo "${HOSTNAME}" > "${METADATA_DIR}/hostname"
    echo "${VMID}" > "${METADATA_DIR}/instance-id"
    echo "${LOCATION}" > "${METADATA_DIR}/region"
    echo "azure" > "${METADATA_DIR}/provider"
}

# Main
log "Detecting cloud provider..."
PROVIDER=$(detect_provider)
log "Detected provider: ${PROVIDER}"

case "${PROVIDER}" in
    aws)   fetch_aws ;;
    gcp)   fetch_gcp ;;
    azure) fetch_azure ;;
    *)     log "No cloud provider detected, skipping metadata fetch" ;;
esac

# Apply hostname if found
if [ -f "${METADATA_DIR}/hostname" ] && [ -s "${METADATA_DIR}/hostname" ]; then
    HOSTNAME=$(cat "${METADATA_DIR}/hostname")
    hostname "${HOSTNAME}" 2>/dev/null || true
    log "Set hostname to: ${HOSTNAME}"
fi

log "Metadata written to ${METADATA_DIR}"
CLOUD_EOF
    chmod +x "${INITRAMFS_DIR}/usr/bin/keel-cloud-init"

    # Write cloud variant marker
    echo "cloud" > "${INITRAMFS_DIR}/etc/keel/variant"
    echo "  Added cloud metadata agent"
fi

# Debug tools (for dev variants)
if [ "${VARIANT_DEBUG_TOOLS:-false}" = "true" ]; then
    echo ">>> Adding debug tools..."

    # Create additional busybox symlinks for debugging
    for tool in ls cat ps top dmesg free df du netstat ss lsof vi ping traceroute \
                nslookup wget tail head wc grep awk sed tr sort uniq tee \
                find xargs env printenv id whoami uptime; do
        ln -sf busybox "${INITRAMFS_DIR}/bin/${tool}" 2>/dev/null || true
    done
    echo "  Added busybox debug applet symlinks"

    # Write dev variant marker
    mkdir -p "${INITRAMFS_DIR}/etc/keel"
    echo "dev" > "${INITRAMFS_DIR}/etc/keel/variant"
    echo "  Debug tools installed"
fi

# Edge minimal mode: remove non-essential components to reduce footprint
if [ "${VARIANT_MINIMAL:-false}" = "true" ]; then
    echo ">>> Applying edge/minimal optimizations..."

    # Remove CNI plugins not commonly needed on edge
    # Keep only: bridge, host-local, loopback, portmap
    for plugin in "${INITRAMFS_DIR}"/opt/cni/bin/*; do
        name=$(basename "$plugin")
        case "$name" in
            bridge|host-local|loopback|portmap) ;;
            *) rm -f "$plugin"; echo "  Removed CNI plugin: $name" ;;
        esac
    done

    # Remove pre-loaded container images to save space (will pull on demand)
    if [ -d "${INITRAMFS_DIR}/usr/share/keel/images" ]; then
        rm -rf "${INITRAMFS_DIR}/usr/share/keel/images"
        echo "  Removed pre-loaded container images"
    fi

    # Write edge variant marker
    mkdir -p "${INITRAMFS_DIR}/etc/keel"
    echo "edge" > "${INITRAMFS_DIR}/etc/keel/variant"
    echo "  Edge optimizations applied"
fi

# Write variant metadata (if not already written)
if [ -n "${VARIANT_NAME:-}" ]; then
    mkdir -p "${INITRAMFS_DIR}/etc/keel"
    if [ ! -f "${INITRAMFS_DIR}/etc/keel/variant" ]; then
        echo "${VARIANT_NAME}" > "${INITRAMFS_DIR}/etc/keel/variant"
    fi
fi

# Create essential devices (if not using devtmpfs)
# sudo mknod -m 600 "${INITRAMFS_DIR}/dev/console" c 5 1
# sudo mknod -m 666 "${INITRAMFS_DIR}/dev/null" c 1 3

echo ">>> Packing initramfs.cpio.gz..."
cd "${INITRAMFS_DIR}"
find . -print0 | cpio --null -ov --format=newc | gzip > "${OUTPUT_DIR}/initramfs.cpio.gz"

echo "Initramfs created at ${OUTPUT_DIR}/initramfs.cpio.gz"
