# Role-Based Access Control (RBAC)

KeelOS enforces role-based access control on all `keel-agent` gRPC endpoints. Roles are derived from client mTLS certificates following Kubernetes RBAC conventions.

## Overview

| Role | Certificate Organization (O) | Access Level |
|------|-------------------------------|-------------|
| **Admin** | `system:masters` or `keel:admin` | Full access to all endpoints |
| **Operator** | `keel:operator` | Operational tasks (updates, snapshots, cert rotation) |
| **Viewer** | `keel:viewer` | Read-only access (status, health, history) |

> **Development mode:** When mTLS is not configured, all requests are allowed regardless of role. This is intended for local development only.

## How It Works

1. A client connects to `keel-agent` via mTLS with a client certificate
2. The agent extracts the **Organization (O)** field from the certificate's subject
3. The Organization field is mapped to an RBAC role
4. Each gRPC endpoint checks the client's role against its required minimum role
5. If the role is insufficient, the request is rejected with `PERMISSION_DENIED`

This follows the same convention used by Kubernetes, where the certificate Organization field maps to groups.

## Endpoint Permissions

### Admin (Full Access)

These endpoints perform dangerous or irreversible operations:

| Endpoint | osctl Command | Description |
|----------|---------------|-------------|
| `Reboot` | `osctl reboot` | Reboot the node |
| `TriggerRollback` | `osctl rollback trigger` | Manually trigger a rollback |
| `BootstrapKubernetes` | `osctl bootstrap` | Join a Kubernetes cluster |
| `ConfigureNetwork` | `osctl network config set` | Modify network configuration |
| `EnableDebugMode` | `osctl diag debug` | Enable time-limited debug mode |
| `EnableRecoveryMode` | `osctl diag recovery` | Enable emergency recovery mode |

### Operator (Operational Tasks)

These endpoints manage updates, diagnostics, and certificates:

| Endpoint | osctl Command | Description |
|----------|---------------|-------------|
| `InstallUpdate` | `osctl update` | Install an OS update |
| `ScheduleUpdate` | *(scheduled updates)* | Schedule an update for a future time |
| `CancelScheduledUpdate` | *(cancel schedule)* | Cancel a pending scheduled update |
| `RotateCertificate` | *(cert rotation)* | Rotate operational certificates |
| `StreamLogs` | `osctl diag logs` | Stream system logs |
| `CollectCrashDump` | `osctl diag crash-dump` | Collect a crash dump |
| `CreateSystemSnapshot` | `osctl diag snapshot` | Create a system snapshot |

### Viewer (Read-Only)

These endpoints return status and historical data:

| Endpoint | osctl Command | Description |
|----------|---------------|-------------|
| `GetStatus` | `osctl status` | Node status |
| `GetHealth` | `osctl health` | Health check results |
| `GetUpdateSchedule` | *(view schedules)* | View pending update schedules |
| `GetRollbackHistory` | `osctl rollback history` | View rollback event history |
| `GetBootstrapStatus` | `osctl bootstrap-status` | K8s bootstrap state |
| `GetNetworkConfig` | `osctl network config show` | View network configuration |
| `GetNetworkStatus` | `osctl network status` | View network interface status |
| `GetDebugStatus` | `osctl diag debug-status` | Check debug mode state |

### Unauthenticated (No RBAC)

| Endpoint | osctl Command | Description |
|----------|---------------|-------------|
| `InitBootstrap` | `osctl init bootstrap` | Initial certificate exchange |

`InitBootstrap` is exempt from RBAC because it is the entry point for establishing mTLS — the client has no certificate yet.

## Setting Up RBAC

### 1. Apply Kubernetes ClusterRoles

The `k8s/rbac.yaml` manifest defines three ClusterRoles matching the keel-agent RBAC roles:

```bash
kubectl apply -f k8s/rbac.yaml
```

This creates:
- `keel-agent-admin` — full access
- `keel-agent-operator` — operational access
- `keel-agent-viewer` — read-only access

### 2. Issue Client Certificates

Client certificates must include the appropriate Organization (O) field. Using `openssl`:

**Admin certificate:**
```bash
openssl req -new -key admin.key \
  -subj "/CN=admin-user/O=keel:admin" \
  -out admin.csr
```

**Operator certificate:**
```bash
openssl req -new -key operator.key \
  -subj "/CN=operator-user/O=keel:operator" \
  -out operator.csr
```

**Viewer certificate:**
```bash
openssl req -new -key viewer.key \
  -subj "/CN=viewer-user/O=keel:viewer" \
  -out viewer.csr
```

Sign these CSRs with your cluster CA or use Kubernetes CertificateSigningRequests.

### 3. Connect with osctl

Once mTLS is configured, `osctl` automatically loads certificates from its local cert store:

```bash
# Admin user can reboot
osctl --endpoint https://<NODE_IP>:50051 reboot

# Viewer user can check status
osctl --endpoint https://<NODE_IP>:50051 status

# Viewer user cannot reboot (PERMISSION_DENIED)
osctl --endpoint https://<NODE_IP>:50051 reboot
# Error: Role 'viewer' does not have permission for this operation (requires 'admin')
```

## Role Hierarchy

Roles are hierarchical — higher roles inherit all permissions of lower roles:

```
Admin  ──► Operator  ──► Viewer
  │            │            │
  │            │            └─ GetStatus, GetHealth, GetNetworkStatus, ...
  │            └─ InstallUpdate, StreamLogs, CollectCrashDump, ...
  └─ Reboot, TriggerRollback, ConfigureNetwork, EnableDebugMode, ...
```

An Admin can perform all Operator and Viewer operations. An Operator can perform all Viewer operations.

## Error Messages

When a client's role is insufficient, `keel-agent` returns a `PERMISSION_DENIED` gRPC status:

```
Role 'viewer' does not have permission for this operation (requires 'admin')
```

When a client certificate has no recognized Organization field:

```
Unrecognized client certificate role: No recognized RBAC role in certificate Organization field
```

## Kubernetes Integration

The Kubernetes ClusterRoles in `k8s/rbac.yaml` document the intended permissions for each role. While the actual enforcement happens in `keel-agent` (not the Kubernetes API server), the ClusterRoles serve as:

1. **Documentation** of the RBAC policy
2. **ClusterRoleBindings** for organizational policy enforcement
3. **Audit trail** via Kubernetes RBAC audit logs

### Binding Users to Roles

```yaml
# Grant admin access to the platform-admin group
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: keel-admin-binding
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: keel-agent-admin
subjects:
- kind: Group
  name: keel:admin
  apiGroup: rbac.authorization.k8s.io
```

```yaml
# Grant operator access to the SRE team
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: keel-operator-binding
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: keel-agent-operator
subjects:
- kind: Group
  name: keel:operator
  apiGroup: rbac.authorization.k8s.io
```

## Security Considerations

- **mTLS required**: RBAC is only enforced when mTLS is active. Without TLS, all requests are allowed.
- **Certificate Organization field**: The O field is the sole source of role identity. Protect your CA signing process.
- **Principle of least privilege**: Assign the minimum role necessary for each user or service account.
- **Audit logging**: All authorization decisions are logged via `tracing` with the client role and required role.
- **No runtime configuration**: Roles are compiled into the agent binary. To change role mappings, update `rbac.rs` and rebuild.

## See Also

- [Certificate Management Guide](../certificate-management.md) — Certificate lifecycle and mTLS setup
- [osctl CLI Reference](../reference/osctl.md) — Complete CLI reference
- [Kubernetes RBAC Manifests](../../k8s/rbac.yaml) — ClusterRole definitions
- [KeelOS Architecture](../architecture.md) — System architecture overview
