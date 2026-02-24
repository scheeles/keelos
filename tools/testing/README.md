# Testing Tools

This directory contains scripts for verification, local development, and end-to-end testing of KeelOS.

## Local Development

### `run-qemu.sh`

**Usage**: `./tools/testing/run-qemu.sh`

Boots the locally built KeelOS artifacts in QEMU.
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

Runs a "smoke test" by booting the OS in QEMU and verifying that it reaches a "ready" state (e.g., `keel-init` started successfully) without panicking.

### `test-integration.sh` / `test-update-flow.sh`

Runs detailed end-to-end scenarios:
*   **Update Flow**: Tests the A/B partition swap and OTA update mechanism.
*   **Integration**: Verifies that `keel-agent` allows `osctl` connections and can spawn containers.

### `test-diagnostics.sh`

End-to-end tests for the diagnostics and debugging tools. Boots KeelOS in QEMU and uses `osctl diag` commands to verify:
*   **Debug Mode**: Enable/disable time-limited debug sessions, verify status, and duplicate rejection.
*   **Recovery Mode**: Enable emergency recovery mode via API.
*   **Crash Dump**: Collect kernel + userspace crash dump and verify output.
*   **System Snapshot**: Create a system snapshot with config and logs.

Each test runs in an isolated QEMU instance with a unique gRPC port.

### `test-audit.sh`

End-to-end tests for the audit logging system. Boots KeelOS in QEMU and verifies that all gRPC API operations are automatically recorded in the audit log:
*   **Audit Log Creation**: Verifies audit entries are generated after API calls.
*   **Multiple Operations**: Confirms that successive API calls each produce audit entries.
*   **Method Name Capture**: Checks that audit entries contain the gRPC method path (e.g. `GetHealth`).
*   **Diagnostic Audit Trail**: Validates that diagnostic operations like `EnableDebugMode` are captured.

Each test runs in an isolated QEMU instance with a unique gRPC port.

## Setup

### `setup-test-disk.sh`

Creates a mock `sda.img` disk image with:
*   GPT Partition Table.
*   EFI System Partition (ESP).
*   Correct partition labels (e.g., `MATIC_STATE`) required by `keel-init` to mount persistent storage.
