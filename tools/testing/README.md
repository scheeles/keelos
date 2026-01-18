# Testing Tools

This directory contains scripts for verification, local development, and end-to-end testing of MaticOS.

## Local Development

### `run-qemu.sh`

**Usage**: `./tools/testing/run-qemu.sh`

Boots the locally built MaticOS artifacts in QEMU.
*   **Kernel**: `build/kernel/bzImage`
*   **Initramfs**: `build/initramfs.cpio.gz`
*   **Disk**: `build/sda.img` (Mock data drive)
*   **Networking**:
    *   Maps host port `50052` to guest port `50051` (gRPC Agent).
    *   Console output is redirected to `stdout`.

## Verification Scripts

### `verify-artifacts.sh`

Checks if all required build artifacts (kernel, initramfs, binary components) exist in the `build/` directory. Useful to run before starting tests.

### `test-boot.sh`

Runs a "smoke test" by booting the OS in QEMU and verifying that it reaches a "ready" state (e.g., `matic-init` started successfully) without panicking.

### `test-integration.sh` / `test-update-flow.sh`

Runs detailed end-to-end scenarios:
*   **Update Flow**: Tests the A/B partition swap and OTA update mechanism.
*   **Integration**: Verifies that `matic-agent` allows `osctl` connections and can spawn containers.

## Setup

### `setup-test-disk.sh`

Creates a mock `sda.img` disk image with:
*   GPT Partition Table.
*   EFI System Partition (ESP).
*   Correct partition labels (e.g., `MATIC_STATE`) required by `matic-init` to mount persistent storage.
