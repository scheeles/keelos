# K8s Integration Notes

## Architecture

**Agent Deployment Model:**
- Agent runs as **system process** (part of KeelOS base image)
- Managed by systemd as a service
- **NOT** a K8s DaemonSet or workload
- Built into the OS, starts on boot

## K8s Integration

When the agent (running as a process) detects it's on a K8s node, it:

1. **Checks for service account token**: `/var/run/secrets/kubernetes.io/serviceaccount/token`
2. **Reads NODE_NAME** from environment variable (set by kubelet)
3. **Uses K8s CSR API** to request operational certificate
4. **Stores cert** in `/var/lib/keel/crypto/operational.{pem,key}`
5. **Uses cert for mTLS** with osctl clients

## RBAC Setup

The agent process needs permissions to create and approve CSRs:

```bash
kubectl apply -f k8s/rbac.yaml
```

This creates:
- **ServiceAccount**: `keel-agent` in `keel-system` namespace
- **ClusterRole**: CSR permissions (create, get, list, approve)
- **ClusterRoleBinding**: Binds role to service account

## How It Works

### On Node Initialization:

```
1. KeelOS boots
2. Systemd starts keel-agent service
3. Agent detects K8s (checks for service account token)
4. Agent requests operational certificate via CSR
5. K8s signs certificate (if RBAC allows)
6. Agent stores and uses cert for mTLS
```

### Service Account Token Mounting:

The kubelet automatically mounts service account tokens to pods, but since the agent is a host process, we have two options:

**Option A:** Manually mount the token (recommended):
```yaml
# In kubelet config or node setup script
--volume=/var/run/secrets/kubernetes.io/serviceaccount:/var/run/secrets/kubernetes.io/serviceaccount:ro
```

**Option B:** Use static kubeconfig:
- Pre-provision `/etc/keel/kubeconfig` with appropriate credentials
- Agent uses this instead of service account token

## Environment Variables

The agent expects:
- `NODE_NAME`: Set by kubelet or init system
- Optional: `KUBECONFIG`: Path to kubeconfig if not using service account

## Testing

```bash
# 1. Apply RBAC
kubectl apply -f k8s/rbac.yaml

# 2. On the node, ensure service account token is available
ls -la /var/run/secrets/kubernetes.io/serviceaccount/

# 3. Set NODE_NAME environment variable
export NODE_NAME=$(hostname)

# 4. Start agent
systemctl start keel-agent

# 5. Check logs for cert initialization
journalctl -u keel-agent -f
# Should see: "✓ K8s operational certificates initialized"
```

## Differences from DaemonSet Approach

| Aspect | Process (Our Approach) | DaemonSet |
|--------|------------------------|-----------|
| Lifecycle | OS-managed (systemd) | K8s-managed |
| Deployment | Built into OS image | Container image |
| Updates | OS update mechanism | K8s rolling update |
| Access | Direct host access | Via hostNetwork/hostPID |
| Security | Native system service | Container security context |
| Bootstrap | Available pre-cluster | Requires cluster |

Our approach is better because:
- ✅ Agent available even if K8s is down
- ✅ Can bootstrap K8s itself
- ✅ Direct disk access (no container overhead)
- ✅ Native systemd integration
- ✅ Part of immutable OS image
