# Kubernetes Bootstrap Guide

This guide describes how to join a KeelOS node to an existing Kubernetes cluster.

## Overview

KeelOS nodes can join Kubernetes clusters using the `osctl bootstrap` command, which configures the kubelet to connect to your cluster's API server. The bootstrap process supports two authentication methods:

1. **Bootstrap Token** - Uses Kubernetes TLS bootstrapping (recommended for production)
2. **Kubeconfig File** - Uses pre-generated credentials (simpler for testing)

## Prerequisites

Before bootstrapping a KeelOS node, you need:

- A running Kubernetes cluster with an accessible API server
- Network connectivity from the KeelOS node to the API server
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
  | base64 -d > /tmp/ca.crt
```

### Step 3: Bootstrap the Node

Transfer the CA certificate to a location accessible by `osctl`, then run:

```bash
osctl --endpoint http://<keelos-node-ip>:50051 bootstrap \
  --api-server https://<k8s-api-server>:6443 \
  --token <bootstrap-token> \
  --ca-cert /tmp/ca.crt
```

**Example:**
```bash
osctl --endpoint http://192.168.1.100:50051 bootstrap \
  --api-server https://k8s.example.com:6443 \
  --token abcdef.0123456789abcdef \
  --ca-cert /tmp/ca.crt
```

### Step 4: Verify Node Joined

Check that the node appears in your cluster:

```bash
kubectl get nodes
```

You should see your KeelOS node listed (it may take 30-60 seconds to appear).

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

By default, the node uses its hostname. To override:

```bash
osctl bootstrap \
  --api-server https://k8s.example.com:6443 \
  --token <token> \
  --ca-cert /tmp/ca.crt \
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

## Troubleshooting

### Node Not Appearing in Cluster

1. **Check kubelet logs:**
   ```bash
   osctl --endpoint http://<keelos-node-ip>:50051 logs --component kubelet
   ```

2. **Verify network connectivity:**
   ```bash
   # From KeelOS node
   curl -k https://<k8s-api-server>:6443/healthz
   ```

3. **Check bootstrap status:**
   ```bash
   osctl --endpoint http://<keelos-node-ip>:50051 bootstrap-status
   ```

### Authentication Errors

**Error: "Unauthorized" or "x509: certificate signed by unknown authority"**

- Verify the CA certificate is correct
- Ensure the bootstrap token is valid and not expired
- Check that the API server endpoint URL is correct

### Token Expired

Bootstrap tokens have a limited lifetime. If your token expired:

1. Create a new token (Step 1 above)
2. Re-run the bootstrap command with the new token

### Kubelet Not Starting

If kubelet fails to start after bootstrap:

1. Check that containerd is running:
   ```bash
   osctl --endpoint http://<keelos-node-ip>:50051 status
   ```

2. Review kubelet configuration:
   ```bash
   # The kubeconfig should exist at:
   # /var/lib/keel/kubernetes/kubelet.kubeconfig
   ```

---

## Security Considerations

- **Bootstrap tokens are sensitive** - Treat them like passwords. Anyone with a valid bootstrap token can add nodes to your cluster.
- **Use short-lived tokens** - Set token duration to the minimum needed (e.g., 1-24 hours).
- **Rotate certificates** - KeelOS kubelet supports automatic certificate rotation when using bootstrap tokens.
- **Network security** - Ensure the API server is only accessible from trusted networks.

---

## How It Works

When you run `osctl bootstrap`:

1. **keel-agent** receives the bootstrap request
2. Creates `/var/lib/keel/kubernetes/` directory
3. Writes the CA certificate to `/var/lib/keel/kubernetes/ca.crt`
4. Generates a kubeconfig file at `/var/lib/keel/kubernetes/kubelet.kubeconfig`
5. Updates kubelet configuration to use the kubeconfig
6. Signals kubelet to restart with new configuration
7. Kubelet connects to the API server and registers the node

The kubeconfig persists across reboots on the `/var/lib/keel` partition.

---

## Next Steps

After bootstrapping:

- **Deploy workloads** - Your KeelOS node is ready to run Kubernetes pods
- **Monitor node health** - Use `kubectl describe node <node-name>`
- **OS updates** - Use `osctl update` to manage OS updates without disrupting cluster membership
- **Decommission** - To remove a node, drain it first: `kubectl drain <node-name> --ignore-daemonsets`

---

## Related Documentation

- [KeelOS Architecture](../learn-more/architecture.md)
- [First Boot Guide](../getting-started/first-boot.md)
- [API Management](../learn-more/api-management.md)
- [Kubernetes TLS Bootstrapping](https://kubernetes.io/docs/reference/access-authn-authz/kubelet-tls-bootstrapping/)
