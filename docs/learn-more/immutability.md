# Immutability & Updates

Immutability is at the core of KeelOS's reliability and security model.

## What is an Immutable OS?

In a traditional OS (like Ubuntu or CentOS), the filesystem is writable. When you run `apt-get upgrade`, the package manager overwrites binaries, libraries, and configuration files in place. If the power fails during an update, your system might end up in a broken, half-updated state.

In KeelOS, the root filesystem is **read-only**.
*   It is delivered as a SquashFS image (a compressed, read-only filesystem).
*   No process (not even root) can modify system files.
*   Configuration is injected at runtime or stored on a separate persistence partition.

## The A/B Update Mechanism

Because we cannot modify the running system, we update it by swapping it out entirely. KeelOS manages two partition slots for the OS: **Root A** and **Root B**.

### The Update Flow

1.  **Download**: The `keel-agent` downloads the new system image (SquashFS) and verifies its signature.
2.  **Write**: The image is written to the *inactive* partition.
    *   If you are currently running on **A**, the update is written to **B**.
3.  **Activate**: The bootloader configuration is updated to set **B** as the next boot target.
4.  **Reboot**: The system reboots.
5.  **Verify**: The node boots into the new version. The `keel-agent` performs health checks (connectivity, kubelet status).
    *   **Success**: The update is marked as successful.
    *   **Failure**: If the node fails to boot or pass health checks, the watchdog automatically reverts the bootloader to **A** and reboots, restoring the system to its previous working state.

## Persistence

While the OS is immutable, Kubernetes needs to store data (container images, logs, etcd data).

KeelOS mounts a specialized writable partition at `/var/lib/keel`.
*   **Persistent**: Data written here survives reboots.
*   **Ephemeral**: Data written to `/tmp` or `/run` is lost on reboot.

This clear separation ensures that "system state" and "application data" never mix.
