# API-Driven Management

The defining feature of KeelOS is that it is managed entirely through an API. We treat the Operating System as a software service, not a collection of config files and scripts.

## The Keel Agent

The `keel-agent` runs on every node. It exposes a gRPC interface on port `50051`. This agent is the *only* way to modify the configuration of the machine.

### Key Capabilities
*   **Status**: Query current version, uptime, and health.
*   **Install**: Stream a new OS image to the inactive partition.
*   **Reboot**: Trigger a safe reboot.
*   **Logs**: Stream logs from `kubelet`, `containerd`, or the system.
*   **Network**: (Planned) Configure static IPs, DNS, and routes.

## osctl: The CLI Client

`osctl` is the reference implementation of a client for the KeelOS API. It translates human-friendly commands into gRPC calls.

```bash
# Get node status
osctl --endpoint https://node-1:50051 status

# Stream kubelet logs
osctl --endpoint https://node-1:50051 logs --component kubelet
```

Because `osctl` talks gRPC, it can be run from anywhere—your laptop, a CI/CD pipeline, or even another Kubernetes pod.

## Security & Authentication

Since the API controls the entire node, securing it is paramount.

### Mutual TLS (mTLS)
KeelOS uses Mutual TLS for authentication. This means:
1.  **Server Identity**: The node presents a certificate to prove it is "node-1".
2.  **Client Identity**: You (`osctl`) must present a certificate signed by a trusted CA to prove you are an authorized administrator.

Currently, in Alpha versions, mTLS may be disabled or optional for testing (see `README`), but it is enforced in production builds.

### Authorization
Future versions of KeelOS will support RBAC, allowing you to grant read-only access to monitoring systems while reserving update privileges for administrators.
