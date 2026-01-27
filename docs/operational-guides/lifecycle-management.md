# Lifecycle Management

Managing the lifecycle of a KeelOS node involves updating the OS image, handling rollbacks, and performing disaster recovery.

## Updates

KeelOS nodes are updated by installing a new OS image to the inactive partition and rebooting.

### 1. Check Current Status
Before updating, verify the node's health and identify the active partition.

```bash
osctl status
# Output:
# Version: 1.0.0
# Active Partition: A
# Health: Healthy
```

### 2. Install Update
Stream the new image to the node. The node will write it to the inactive partition (Partition B in this case).

```bash
osctl install \
  --image oci://registry.example.com/keelos:1.1.0 \
  --verify-signature
```

### 3. Reboot
Trigger a reboot to switch to the new partition.

```bash
osctl reboot
```

## Delta Updates (Bandwidth-Optimized)

For minor version updates, KeelOS supports **Delta Updates**. Instead of downloading the full OS image (~100MB+), the agent downloads a small binary patch file (often <10MB) containing only the differences between the running version and the new version.

### 1. Generate Delta File
Delta files are generated on your build server using the `generate-delta.sh` tool. This tool uses `bsdiff` to compare two SquashFS images.

```bash
# syntax: generate-delta.sh OLD_IMAGE NEW_IMAGE OUTPUT_DELTA
./tools/builder/generate-delta.sh os-v1.0.squashfs os-v1.1.squashfs update-v1.0-to-v1.1.delta
```

### 2. Apply Delta Update
Use the `--delta` flag with `osctl update`. You MUST provide a `--fallback` URL (the full image) in case the delta application fails (e.g., mismatching base version).

```bash
osctl update \
  --source http://update-server/update-v1.0-to-v1.1.delta \
  --delta \
  --fallback \
  --full-image-url http://update-server/os-v1.1.squashfs
```

### How it Works
1.  **Download**: The agent downloads the small delta file.
2.  **Patch**: It reads the *active* partition (v1.0), applies the patch in memory, and writes the resulting v1.1 image to the *inactive* partition.
3.  **Verify**: It calculates the SHA256 of the new image and compares it against the expected hash (if provided).
4.  **Fallback**: If patching fails or checksums don't match, the agent automatically downloads the full image from `full_image_url`.


KeelOS supports both automatic and manual rollbacks to ensure high availability.

### Automatic Rollback
When a node reboots into a new version, it enters a "probationary" period. The `keel-agent` runs a set of health checks (API connectivity, kubelet status).
*   **Success**: If checks pass, the new version is marked as "committed."
*   **Failure**: If checks fail or the system crashes, the hardware watchdog or the bootloader logic reverts the active partition to the previous version and reboots.

### Manual Rollback
If you discover an issue after the update is committed (e.g., application performance degradation), you can manually trigger a rollback.

```bash
# Trigger an immediate rollback
osctl rollback trigger --reason "Performance regression in v1.1.0"
```

## Disaster Recovery

If a node becomes completely unreachable via API and fails to auto-rollback (e.g., severe hardware error or corrupted bootloader), you may need to intervene physically or via IPMI/console.

### 1. Console Access
Since there is no shell, the console logs are your primary source of truth. Check for kernel panics or filesystem errors.

### 2. Force Boot Previous Partition
Most bootloaders (GRUB/Limine) allow you to select a partition at boot time.
1.  Reboot the machine.
2.  Intervene at the boot menu.
3.  Select "KeelOS (Rollback)" or the entry corresponding to the previous partition.
