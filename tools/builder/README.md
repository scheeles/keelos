# Builder Tools

The `builder` directory provides a hermetic build environment for MaticOS, packaged as a Docker container. This ensures consistent builds across different host operating systems (macOS, Linux, Windows).

## Scripts

### `build.sh`

**Usage**: `./tools/builder/build.sh`

This is the main entry point. It:
1.  Builds the `maticos-builder` Docker image (defined in `Dockerfile` in this directory).
2.  Launches an interactive shell inside the container.
3.  Mounts the project root to `/maticos`.
4.  Caches `~/.cargo/registry` and `/maticos/target` to host volumes for faster subsequent builds.

**Environment**:
Inside the container, you have access to:
*   `cargo` / `rustc` (Nightly/Stable)
*   `gcc` (Cross-compilers)
*   `make`, `bison`, `flex` (Kernel build tools)
*   `protobuf-compiler` (for gRPC)

### `kernel-build.sh`

**Usage**: `./tools/builder/kernel-build.sh` (Inside the builder container)

Automates the Linux Kernel build process:
1.  **Downloads**: Fetches the specified Linux Kernel tarball (e.g., 6.6.x).
2.  **Configures**: Applies a minimal `x86_64_defconfig` and enables critical MaticOS features:
    *   `CONFIG_SQUASHFS`: For the immutable root filesystem.
    *   `CONFIG_OVERLAY_FS`: For container storage.
    *   `CONFIG_BLK_DEV_INITRD`: For initramfs support.
    *   `CONFIG_PVH`: For efficient booting in QEMU/Firecracker.
3.  **Builds**: Compiles the kernel using `make bzImage`.
4.  **Artifact**: Outputs `build/kernel/bzImage`.

### `initramfs-build.sh`

**Usage**: `./tools/builder/initramfs-build.sh` (Inside the builder container)

Assembles the initial RAM filesystem used by the kernel at boot.
1.  **Directory Structure**: Creates the standard Linux hierarchy (`/bin`, `/etc`, `/proc`, etc.).
2.  **Binaries**:
    *   Copies `matic-init` (PID 1) to `/init`.
    *   Copies `matic-agent` and `osctl` to `/usr/bin/`.
    *   Copies `containerd`, `runc`, and CNI plugins.
    *   Copies statically linked `busybox` for debugging shell.
3.  **Libraries**: Copies required GLIBC libraries for dynamic binaries (like stock `kubelet`).
4.  **Packaging**: Packs the directory tree into `build/initramfs.cpio.gz`.

## Build Artifacts

All artifacts are output to the `build/` directory in the project root:

*   `build/kernel/bzImage`: The bootable kernel.
*   `build/initramfs.cpio.gz`: The root filesystem.
