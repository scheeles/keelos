# Troubleshooting

This guide covers common issues when running KeelOS and how to resolve them.

## Boot Issues

### System Doesn't Boot

**Symptom:** No output on serial console or the bootloader menu appears but KeelOS fails to start.

**Possible Causes & Solutions:**

1. **Wrong boot mode**: Ensure UEFI/BIOS matches the image type. UEFI is recommended.
2. **Secure Boot enabled**: Disable Secure Boot (KeelOS kernel signing keys are not yet distributed).
3. **Corrupted image**: Re-download the image and verify the checksum:
   ```bash
   sha256sum -c SHA256SUMS
   ```
4. **Kernel panic at boot**: Try the debug boot entry from the GRUB/Limine menu for verbose output. Look for missing hardware drivers or filesystem errors.

### Boot Loop (Repeated Reboots)

**Symptom:** Node reboots repeatedly without stabilizing.

**Possible Causes & Solutions:**

1. **Failed update with auto-rollback**: The automatic rollback supervisor may be cycling between partitions. After three failed boots, the system enters recovery mode.
2. **Kernel panic configuration**: KeelOS sets `panic=1`, causing an immediate reboot on kernel panic. Connect a serial console to capture the panic message before the reboot.
3. **Data partition corruption**: If the persistent data partition is corrupted, `keel-init` may fail during early boot. Recreate the data partition by deleting and reformatting it (data will be lost).

---

## Connectivity Issues

### Can't Connect to keel-agent

**Symptom:** `osctl` returns "Connection Refused" or times out.

**Possible Causes & Solutions:**

1. **Agent hasn't started yet**: The agent takes a few seconds after boot. Watch the serial console for `Starting keel-agent...` before connecting.
2. **Wrong endpoint**: Verify the node IP and port. The default is `50051`. When using QEMU with `run-qemu.sh`, use `localhost:50052` (port forwarding).
   ```bash
   osctl --endpoint http://127.0.0.1:50052 status
   ```
3. **Firewall blocking port 50051**: Ensure your network allows TCP traffic to port 50051 on the node.
4. **Network not configured**: If the node has no IP address, check that DHCP is available or configure a static IP. See the [Networking guide](./learn-more/networking.md).

### mTLS Certificate Errors

**Symptom:** `osctl` returns "certificate has expired" or "unknown authority" errors.

**Possible Causes & Solutions:**

1. **Bootstrap certificate expired** (24-hour validity):
   ```bash
   osctl init bootstrap --node <NODE_IP>
   ```
2. **Operational certificate not initialized**: If running in Kubernetes, ensure RBAC is configured and the `NODE_NAME` environment variable is set. See the [Certificate Management guide](./certificate-management.md).
3. **CA mismatch**: The client certificate must be signed by a CA that the agent trusts. Verify that the correct CA is loaded.

---

## Update Issues

### Update Fails to Download

**Symptom:** `osctl update` reports a download error.

**Possible Causes & Solutions:**

1. **No outbound network access**: The node must be able to reach the update server URL. Check DNS and proxy settings.
2. **Invalid URL**: Verify the `--source` URL is correct and accessible.
3. **Checksum mismatch**: If `--sha256` is provided, the downloaded image must match exactly. Verify the hash on the server side.

### Delta Update Fails

**Symptom:** Delta update returns an error during patching.

**Possible Causes & Solutions:**

1. **Base version mismatch**: Delta patches must be applied against the exact base version they were generated from. Verify the active partition version matches.
2. **Corrupted delta file**: Re-download the delta file and retry.
3. **Fallback triggered**: If `--fallback` was specified, the agent automatically downloads the full image. Check the update progress output for "Falling back to full image".

### Update Stuck at "Running"

**Symptom:** The schedule status shows `running` but never completes.

**Possible Causes & Solutions:**

1. **Slow download**: Large images over slow networks take time. Check the progress percentage.
2. **Disk write error**: The inactive partition may be corrupted or full. Check serial console logs for I/O errors.
3. **Agent crashed during update**: If the agent restarts, the schedule remains in `running` state. Cancel it and retry:
   ```bash
   osctl schedule cancel --id <schedule-id>
   ```

---

## Rollback Issues

### Automatic Rollback Not Triggering

**Symptom:** Node boots into a broken update but doesn't roll back.

**Possible Causes & Solutions:**

1. **Auto-rollback not enabled**: The update must have `enable_auto_rollback` set to `true`. Verify with:
   ```bash
   osctl rollback history
   ```
2. **Health checks passing**: The rollback supervisor only triggers on `unhealthy` status. If health checks pass despite application issues, consider scheduling updates with a longer `--health-check-timeout`.
3. **Grace period**: After boot, the rollback supervisor waits for a configurable grace period (default: 60 seconds) before running health checks. The system may not have failed yet.

### Manual Rollback Fails

**Symptom:** `osctl rollback trigger` returns `success: false`.

**Possible Causes & Solutions:**

1. **No previous partition**: Rollback requires a previous update. If the node has never been updated, there is no partition to roll back to.
2. **Partition data missing**: The rollback state file may be corrupted. Check serial console logs for errors.

---

## Kubernetes Issues

### Node Not Appearing in Cluster

**Symptom:** After running `osctl bootstrap`, the node doesn't appear in `kubectl get nodes`.

**Possible Causes & Solutions:**

1. **Check bootstrap status**:
   ```bash
   osctl --endpoint http://<NODE_IP>:50051 bootstrap-status
   ```
2. **API server unreachable**: Verify network connectivity between the node and the Kubernetes API server (port 6443).
3. **Token expired**: Bootstrap tokens have limited lifetime. Create a new token and re-run the bootstrap command.
4. **Missing persistent storage**: Without the data disk, kubelet state is lost on reboot, causing `DiskPressure`. Ensure `/data` is mounted.

For detailed bootstrap troubleshooting, see the [Kubernetes Bootstrap Guide](./guides/kubernetes-bootstrap.md#troubleshooting).

### Kubelet Not Starting

**Symptom:** Node is bootstrapped but kubelet is not running.

**Possible Causes & Solutions:**

1. **containerd not running**: Kubelet requires containerd. Check node status:
   ```bash
   osctl --endpoint http://<NODE_IP>:50051 status
   ```
2. **Missing kubeconfig**: Verify the bootstrap kubeconfig exists at `/var/lib/keel/kubernetes/kubelet.kubeconfig`.
3. **Disk full**: Container images fill up disk space quickly. Ensure the persistent data partition is large enough (10 GB+ recommended).

---

## Health Check Issues

### Node Reports "Unhealthy"

**Symptom:** `osctl health` returns status `unhealthy`.

**Possible Causes & Solutions:**

1. **Identify the failing check**: Look at the individual check results in the health response. Each check reports `pass`, `fail`, or `unknown`.
2. **Boot check failing**: If the `boot` check fails, the system may have just started (uptime < 10 seconds). Wait and retry.
3. **Network check failing**: Verify the node has at least one active network interface with an IP address.
4. **API check failing**: The gRPC port (50051) should be listening. If not, the agent may have crashed.

### Health Check Timeout

**Symptom:** Updates fail because health checks time out after reboot.

**Possible Causes & Solutions:**

1. **Timeout too short**: Increase the health check timeout when scheduling updates:
   ```bash
   osctl schedule update \
     --source <url> \
     --enable-auto-rollback \
     --health-check-timeout 600
   ```
2. **Slow boot**: Some hardware takes longer to initialize. Consider increasing the grace period.

---

## QEMU/Development Issues

### "QEMU not found"

**Solution:** Install QEMU for your platform:
- **macOS**: `brew install qemu`
- **Ubuntu/Debian**: `sudo apt install qemu-system-x86`
- **Fedora**: `sudo dnf install qemu-system-x86`

### "Connection Refused" in QEMU

**Solution:** Wait for the VM to fully boot. Watch the console output for:
```
[   OK  ] Starting keel-agent...
```
Then connect using the forwarded port:
```bash
osctl --endpoint http://127.0.0.1:50052 status
```

### Resetting QEMU State

To start with a clean VM, delete the disk image:
```bash
rm build/sda.img
./tools/testing/run-qemu.sh
```

---

## Getting More Help

- Check [serial console output](#boot-issues) for kernel and init messages
- Review the [Architecture](./learn-more/architecture.md) to understand the boot sequence
- See the [Certificate Management](./certificate-management.md) guide for mTLS issues
- See the [Network API Reference](./reference/network-api.md) for network configuration
