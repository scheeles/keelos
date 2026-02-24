#!/bin/bash
set -e

KERNEL_VERSION="6.6.14"
KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
CACHE_DIR="/keelos/.cache/kernel"
# Build in ephemeral container FS which is case-sensitive (fixes Mac host mount issues)
BUILD_DIR="/tmp/kernel-build"
SRC_DIR="${BUILD_DIR}/linux-${KERNEL_VERSION}"
OUTPUT_DIR="${VARIANT_KERNEL_OUTPUT_DIR:-/keelos/build/kernel}"

mkdir -p "${CACHE_DIR}"
mkdir -p "${BUILD_DIR}"
mkdir -p "${OUTPUT_DIR}"

# Skip build if kernel already exists (e.g., from cache)
if [ -f "${OUTPUT_DIR}/bzImage" ]; then
    echo ">>> Kernel already exists at ${OUTPUT_DIR}/bzImage, skipping build"
    exit 0
fi

echo ">>> Checking for Kernel Source Tarball..."
if [ ! -f "${CACHE_DIR}/linux.tar.xz" ]; then
    echo "Downloading Kernel ${KERNEL_VERSION}..."
    wget -c "${KERNEL_URL}" -O "${CACHE_DIR}/linux.tar.xz"
fi

echo ">>> Extracting Source to Ephemeral Build Dir..."
# We extract every time to ensure a clean source tree on a proper filesystem
if [ -d "${SRC_DIR}" ]; then
    rm -rf "${SRC_DIR}"
fi
tar -xf "${CACHE_DIR}/linux.tar.xz" -C "${BUILD_DIR}"

cd "${SRC_DIR}"

echo ">>> Configuring Kernel..."
# Clean up any potential stale objects from wrong architecture
make mrproper

# Start with a minimal x86_64 configuration
make ARCH=x86_64 CROSS_COMPILE=x86_64-linux-gnu- x86_64_defconfig

# Enforce our specific requirements (example: SquashFS, OverlayFS)
# In a real scenario, we would merge a fragment from /keelos/kernel/config/base.config
./scripts/config --enable CONFIG_SQUASHFS
./scripts/config --enable CONFIG_SQUASHFS_XZ
./scripts/config --enable CONFIG_OVERLAY_FS
./scripts/config --enable CONFIG_BLK_DEV_INITRD
./scripts/config --enable CONFIG_DEVTMPFS
./scripts/config --enable CONFIG_DEVTMPFS_MOUNT
./scripts/config --enable CONFIG_PVH # For QEMU/Firecracker

# Hardening (Breaks build if not careful, enabling basics)
./scripts/config --enable CONFIG_X86_64
./scripts/config --disable CONFIG_DEBUG_INFO

# eBPF support (required for cgroup v2 device filtering in runc/containerd)
./scripts/config --enable CONFIG_BPF
./scripts/config --enable CONFIG_BPF_SYSCALL
./scripts/config --enable CONFIG_CGROUP_BPF
./scripts/config --enable CONFIG_CGROUP_DEVICE

# Cgroup v2 controllers (required for kubelet/runc resource management)
./scripts/config --enable CONFIG_CGROUPS
./scripts/config --enable CONFIG_MEMCG
./scripts/config --enable CONFIG_CGROUP_PIDS
./scripts/config --enable CONFIG_CGROUP_SCHED
./scripts/config --enable CONFIG_CFS_BANDWIDTH
./scripts/config --enable CONFIG_CPUSETS
./scripts/config --enable CONFIG_BLK_CGROUP

# Namespaces (required for container isolation)
./scripts/config --enable CONFIG_NAMESPACES
./scripts/config --enable CONFIG_NET_NS
./scripts/config --enable CONFIG_PID_NS
./scripts/config --enable CONFIG_IPC_NS
./scripts/config --enable CONFIG_UTS_NS
./scripts/config --enable CONFIG_USER_NS

# Networking (required for CNI/pod networking)
./scripts/config --enable CONFIG_VETH
./scripts/config --enable CONFIG_BRIDGE
./scripts/config --enable CONFIG_BRIDGE_NETFILTER
./scripts/config --enable CONFIG_NETFILTER
./scripts/config --enable CONFIG_NETFILTER_ADVANCED
./scripts/config --enable CONFIG_NETFILTER_XTABLES
./scripts/config --enable CONFIG_NF_CONNTRACK
./scripts/config --enable CONFIG_NF_NAT
./scripts/config --enable CONFIG_NF_TABLES
./scripts/config --enable CONFIG_NF_TABLES_IPV4
./scripts/config --enable CONFIG_NF_TABLES_IPV6
./scripts/config --enable CONFIG_IP_NF_IPTABLES
./scripts/config --enable CONFIG_IP_NF_NAT
./scripts/config --enable CONFIG_IP_NF_FILTER
./scripts/config --enable CONFIG_IP6_NF_IPTABLES
./scripts/config --enable CONFIG_IP6_NF_FILTER
./scripts/config --enable CONFIG_IP6_NF_NAT

# Container runtime support (required by runc/containerd-shim)
./scripts/config --enable CONFIG_SECCOMP
./scripts/config --enable CONFIG_SECCOMP_FILTER
./scripts/config --enable CONFIG_FHANDLE
./scripts/config --enable CONFIG_TMPFS
./scripts/config --enable CONFIG_CGROUP_FREEZER
./scripts/config --enable CONFIG_PROC_FS
./scripts/config --enable CONFIG_POSIX_MQUEUE
./scripts/config --enable CONFIG_KEYS
./scripts/config --enable CONFIG_EPOLL
./scripts/config --enable CONFIG_SIGNALFD
./scripts/config --enable CONFIG_TIMERFD

# =============================================================================
# Variant-specific kernel configuration
# =============================================================================
# Applied via VARIANT_KERNEL_EXTRA_ENABLE / VARIANT_KERNEL_EXTRA_DISABLE env vars
# set by build-variant.sh when building image variants.

if [ -n "${VARIANT_KERNEL_EXTRA_ENABLE:-}" ]; then
    echo ">>> Applying variant kernel configs (enable)..."
    for config in ${VARIANT_KERNEL_EXTRA_ENABLE}; do
        config=$(echo "$config" | xargs)  # trim whitespace
        if [ -n "$config" ]; then
            ./scripts/config --enable "$config"
        fi
    done
fi

if [ -n "${VARIANT_KERNEL_EXTRA_DISABLE:-}" ]; then
    echo ">>> Applying variant kernel configs (disable)..."
    for config in ${VARIANT_KERNEL_EXTRA_DISABLE}; do
        config=$(echo "$config" | xargs)  # trim whitespace
        if [ -n "$config" ]; then
            ./scripts/config --disable "$config"
        fi
    done
fi

# Override debug info setting if variant specifies it
if [ "${VARIANT_KERNEL_DEBUG_INFO:-false}" = "true" ]; then
    echo ">>> Enabling kernel debug info for variant..."
    ./scripts/config --enable CONFIG_DEBUG_INFO
fi

# Update config to resolve any new dependencies non-interactively
make ARCH=x86_64 CROSS_COMPILE=x86_64-linux-gnu- olddefconfig

echo ">>> Building Kernel (bzImage)..."
make -j$(nproc) ARCH=x86_64 CROSS_COMPILE=x86_64-linux-gnu- bzImage

echo ">>> Copying Artifacts..."
cp arch/x86/boot/bzImage "${OUTPUT_DIR}/bzImage"
echo "Kernel available at ${OUTPUT_DIR}/bzImage"
