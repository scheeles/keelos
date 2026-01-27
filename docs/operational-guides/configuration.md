# Configuration

KeelOS moves configuration away from files (`/etc/`) and into the API. While files can still be used for static configuration (via cloud-init style injection or persistent storage), the API is the preferred method for runtime changes.

## Health Checks

The health check framework determines when a node is "healthy" and when it should rollback.

### Configuring Timeouts
You can adjust the timeout for health checks during an update.

```bash
# Set a 10-minute timeout for the update verification
osctl schedule update \
  --image ... \
  --health-check-timeout 600
```

### Custom Checks
(Planned) Future versions will allow you to define custom health checks (e.g., "ping this internal service") via the API.

## Kubelet Configuration

The `kubelet` is the primary service running on KeelOS. Its configuration determines how the node interacts with the Kubernetes cluster.

### Flags
Kubelet flags are typically baked into the image or injected via the kernel command line during PXE boot.

**Example Kernel Arguments:**
```text
keel.kubelet.node-labels=topology.kubernetes.io/region=us-east-1
keel.kubelet.register-with-taints=special=true:NoSchedule
```

### Dynamic Configuration
KeelOS supports [Kubernetes Dynamic Kubelet Configuration](https://kubernetes.io/docs/tasks/administer-cluster/reconfigure-kubelet/), allowing you to manage kubelet settings via the Kubernetes API itself, which `keel-agent` will respect.

## Runtime Configuration

`containerd` acts as the container runtime.

### Registries & Mirrors
To configure private registries or mirrors (e.g., for air-gapped environments), you currently need to mount a custom `config.toml` to `/etc/containerd/config.toml` via the persistent partition or built into a custom OS image.

**Planned API:**
```bash
osctl config containerd registry add \
  --hostname private-registry.corp \
  --mirror http://mirror.local
```
