# Kubernetes Bootstrap Guide

This guide describes how to join a KeelOS node to an existing Kubernetes cluster using kubelet TLS bootstrapping.

## Overview

KeelOS nodes can join Kubernetes clusters using the `osctl bootstrap` command, which configures the kubelet to connect to your cluster's API server. The bootstrap process supports two authentication methods:

1. **Bootstrap Token** — Uses Kubernetes TLS bootstrapping (recommended for production)
2. **Kubeconfig File** — Uses pre-generated credentials (simpler for testing)

## Prerequisites

Before bootstrapping a KeelOS node, ensure the following:

- A running Kubernetes cluster with an accessible API server
- Network connectivity from the KeelOS node to the API server
- A persistent data disk attached to the node (for container images, kubelet state, and bootstrap config)
- Either:
  - A bootstrap token and cluster CA certificate (for token-based auth)
  - OR a valid kubeconfig file with node credentials

## Method 1: Bootstrap with Token (Recommended)

This method uses Kubernetes TLS bootstrapping, which is secure and supports automatic certificate rotation.

### Step 1: Create a Bootstrap Token

On your Kubernetes control plane, create a bootstrap token:

```bash
# Create a token that lasts 24 hours
kubectl create token node-bootstrapper \
  --duration=24h \
  --namespace=kube-system
```

Save the output token (format: `<token-id>.<token-secret>`).

### Step 2: Extract Cluster CA Certificate

Get the cluster's CA certificate:

```bash
kubectl config view --raw \
  -o jsonpath='{.clusters[0].cluster.certificate-authority-data}' \
  | base64 -d > ca.crt
```

### Step 3: Bootstrap the Node

Transfer the CA certificate to a location accessible by `osctl`, then run:

```bash
osctl --endpoint http://<keelos-node-ip>:50051 bootstrap \
  --api-server https://<k8s-api-server>:6443 \
  --token <bootstrap-token> \
  --ca-cert ca.crt
```

**Example:**
```bash
osctl --endpoint http://192.168.1.100:50051 bootstrap \
  --api-server https://k8s.example.com:6443 \
  --token abcdef.0123456789abcdef \
  --ca-cert ca.crt
```

### Step 4: Verify Node Joined

Check that the node appears in your cluster:

```bash
kubectl get nodes
```

You should see your KeelOS node listed (it may take 30–60 seconds to appear).

---

## Method 2: Bootstrap with Kubeconfig

This method uses a pre-generated kubeconfig file, which is simpler but requires manual certificate management.

### Step 1: Generate Kubeconfig

Create a kubeconfig with node credentials:

```bash
# Using kubectl
kubectl config view --raw > keelos-node.kubeconfig
```

Or create a custom kubeconfig with appropriate RBAC permissions for a node.

### Step 2: Bootstrap the Node

```bash
osctl --endpoint http://<keelos-node-ip>:50051 bootstrap \
  --api-server https://<k8s-api-server>:6443 \
  --kubeconfig /path/to/keelos-node.kubeconfig
```

**Example:**
```bash
osctl --endpoint http://192.168.1.100:50051 bootstrap \
  --api-server https://k8s.example.com:6443 \
  --kubeconfig ./keelos-node.kubeconfig
```

---

## Advanced Options

### Override Node Name

By default, the node uses its hostname (or a name from `bootstrap.json` if previously bootstrapped). To override:

```bash
osctl --endpoint http://<keelos-node-ip>:50051 bootstrap \
  --api-server https://k8s.example.com:6443 \
  --token <token> \
  --ca-cert ca.crt \
  --node-name keelos-worker-01
```

### Check Bootstrap Status

Verify the bootstrap configuration:

```bash
osctl --endpoint http://<keelos-node-ip>:50051 bootstrap-status
```

This shows:
- Whether the node is bootstrapped
- API server endpoint
- Node name
- Kubeconfig path
- Bootstrap timestamp

---

## How It Works

When you run `osctl bootstrap`, the following sequence occurs:

### 1. Bootstrap Request (osctl → keel-agent)

1. `osctl` reads the CA certificate and/or kubeconfig file from local disk
2. Sends a `BootstrapKubernetesRequest` to the `keel-agent` gRPC server
3. `keel-agent` validates inputs (API server required, token+CA or kubeconfig required)

### 2. Configuration Persistence (keel-agent)

4. Creates the Kubernetes directory at `/var/lib/keel/kubernetes/`
5. Writes the CA certificate to `/var/lib/keel/kubernetes/ca.crt`
6. Generates (from token) or writes (from file) the kubeconfig to `/var/lib/keel/kubernetes/kubelet.kubeconfig`
7. Saves bootstrap state to `/var/lib/keel/kubernetes/bootstrap.json` (records API server, node name, kubeconfig path, and timestamp)
8. Creates the restart signal file at `/run/keel/restart-kubelet`

### 3. Kubelet Restart (keel-init supervision loop)

9. `keel-init`'s supervision loop detects the restart signal
10. Stops the running kubelet process
11. Restarts kubelet with `--bootstrap-kubeconfig=/var/lib/keel/kubernetes/kubelet.kubeconfig`

### 4. TLS Bootstrap (kubelet → API server)

12. Kubelet uses the bootstrap token to authenticate with the API server
13. Kubelet submits a Certificate Signing Request (CSR) for a permanent client certificate
14. Once the CSR is approved, kubelet writes its permanent kubeconfig to `/var/lib/kubelet/kubeconfig`
15. `keel-init` detects the permanent kubeconfig and restarts kubelet one final time to switch from bootstrap to permanent credentials

### Key Paths

| Path | Purpose |
|------|---------|
| `/var/lib/keel/kubernetes/kubelet.kubeconfig` | Bootstrap kubeconfig (token-based, temporary) |
| `/var/lib/keel/kubernetes/ca.crt` | Cluster CA certificate |
| `/var/lib/keel/kubernetes/bootstrap.json` | Bootstrap state (API server, node name, timestamp) |
| `/var/lib/kubelet/kubeconfig` | Permanent kubeconfig (post-CSR, long-lived) |
| `/var/lib/kubelet/pki/` | Kubelet client certificates |
| `/run/keel/restart-kubelet` | Restart signal file |
| `/etc/kubernetes/kubelet-config.yaml` | Kubelet configuration |

### Persistent Storage

All bootstrap configuration is stored under `/var/lib/keel/`, which is bind-mounted to persistent storage (`/data/keel/` on the data disk). This ensures bootstrap configuration survives reboots.

---

## Troubleshooting

### Node Not Appearing in Cluster

1. **Check bootstrap status:**
   ```bash
   osctl --endpoint http://<keelos-node-ip>:50051 bootstrap-status
   ```

2. **Verify network connectivity:**
   ```bash
   # From a machine that can reach the KeelOS node
   curl -k https://<k8s-api-server>:6443/healthz
   ```

3. **Check that persistent storage is mounted:**
   The data disk must be available so kubelet state persists. Without it, container images and kubelet data live in RAM and fill up quickly, causing `DiskPressure`.

### Authentication Errors

**Error: "Unauthorized" or "x509: certificate signed by unknown authority"**

- Verify the CA certificate is correct and matches the cluster
- Ensure the bootstrap token is valid and not expired
- Check that the API server endpoint URL is correct (include the port)

### Token Expired

Bootstrap tokens have a limited lifetime. If your token expired:

1. Create a new token (Step 1 above)
2. Re-run the bootstrap command with the new token

### Kubelet Not Starting

If kubelet fails to start after bootstrap:

1. **Check that containerd is running.** Kubelet requires containerd for CRI operations. Verify the node status:
   ```bash
   osctl --endpoint http://<keelos-node-ip>:50051 status
   ```

2. **Verify kubeconfig exists:**
   The bootstrap kubeconfig should be at `/var/lib/keel/kubernetes/kubelet.kubeconfig`.

3. **Check persistent storage:**
   If the data disk is not mounted, kubelet will run out of disk space for container images. Ensure `/data` is mounted and the bind-mounts for `/var/lib/containerd`, `/var/lib/kubelet`, and `/var/lib/keel` are active.

---

## Security Considerations

- **Bootstrap tokens are sensitive** — Treat them like passwords. Anyone with a valid bootstrap token can add nodes to your cluster.
- **Use short-lived tokens** — Set token duration to the minimum needed (e.g., 1–24 hours).
- **Rotate certificates** — KeelOS kubelet supports automatic certificate rotation when using bootstrap tokens.
- **Network security** — Ensure the API server is only accessible from trusted networks.
- **mTLS for osctl** — Use `osctl init bootstrap --node <ip>` to enable mTLS between your workstation and the KeelOS node, preventing unauthorized management access.

---

## Next Steps

After bootstrapping:

- **Deploy workloads** — Your KeelOS node is ready to run Kubernetes pods
- **Monitor node health** — Use `kubectl describe node <node-name>`
- **OS updates** — Use `osctl update` to manage OS updates without disrupting cluster membership
- **Decommission** — To remove a node, drain it first: `kubectl drain <node-name> --ignore-daemonsets`

---

## Related Documentation

- [KeelOS Architecture](../learn-more/architecture.md)
- [First Boot Guide](../getting-started/first-boot.md)
- [osctl Reference](../reference/osctl.md)
- [Kubernetes TLS Bootstrapping](https://kubernetes.io/docs/reference/access-authn-authz/kubelet-tls-bootstrapping/)
