# Getting Started with MaticOS

This guide walks you through getting MaticOS running locally.

## Quick Start

**Option 1: Download Pre-built Image** (Recommended)

Download the latest ISO from [GitHub Releases](https://github.com/scheeles/maticos/releases) and boot it:

```bash
qemu-system-x86_64 -cdrom maticos-0.1.0.iso -m 2G -serial stdio
```

For full installation options (VMs, PXE, etc.), see [Installation Guide](./installation.md).

**Option 2: Build from Source**

Continue reading below to build MaticOS from source.

---

## Prerequisites

*   **Docker**: Required to run the hermetic build environment.
*   **QEMU**: Required for local testing (`qemu-system-x86_64`).
*   A host machine with at least 4GB RAM and 10GB free disk space.

## Step 1: Clone the Repository

```bash
git clone https://github.com/scheeles/maticos.git
cd maticos
```

## Step 2: Enter the Build Environment

The build environment is a Docker container with all necessary toolchains pre-installed.

```bash
./tools/builder/build.sh
```

This will:
1.  Build the `maticos-builder` Docker image (first run only).
2.  Drop you into an interactive shell inside the container.

## Step 3: Build the Kernel

Inside the container, run:

```bash
./tools/builder/kernel-build.sh
```

This downloads the Linux kernel source, applies MaticOS-specific configuration, and compiles `bzImage`.

**Output**: `build/kernel/bzImage`

## Step 4: Build the Rust Components

Still inside the container:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

This compiles `matic-init`, `matic-agent`, and `osctl`.

## Step 5: Build the Initramfs

```bash
./tools/builder/initramfs-build.sh
```

This assembles all binaries, libraries, and configurations into a bootable initramfs.

**Output**: `build/initramfs.cpio.gz`

## Step 6: Create a Test Disk

```bash
./tools/testing/setup-test-disk.sh
```

This creates a mock disk image (`build/sda.img`) with the required partition layout.

## Step 7: Boot in QEMU

Exit the Docker container (type `exit`) and run on your host:

```bash
./tools/testing/run-qemu.sh
```

You should see the MaticOS boot sequence. The system will initialize and start `matic-agent`.

## Next Steps

*   [Using osctl](./using-osctl.md) - Learn how to interact with a running MaticOS node.
*   [Architecture Overview](./architecture.md) - Understand how MaticOS is designed.
