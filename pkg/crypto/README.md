# pkg/crypto (Matic Identity)

**Responsibility**: mTLS, Certificate Generation, and Identity management.

KeelOS rejects SSH. All administrative access happens via a gRPC API secured with mutual TLS (mTLS).

## mTLS Strategy

1.  **Trust Root**: The node trusts a specific Root CA (e.g., provided during cluster provisioning or generated at first boot).
2.  **Node Identity**: The `keel-agent` presents a Server Certificate signed by the CA.
3.  **Client Identity**: The `osctl` client (or KubeOne/Control Plane) must present a Client Certificate signed by the same CA.
4.  **Verification**: Both sides verify each other's certificates against the trusted Root CA.

## Implementation Plan

*   **Libraries**: `rustls`, `rcgen` (for certificate generation in tests/bootstrap).
*   **Storage**: Certificates are stored in the encrypted persistence partition (`/var/lib/keel/crypto`).
*   **Initial Boot**: If no certificates exist, the agent will generate a CSR or a temporary self-signed identity (configurable).

## Directory Structure
*   `src/lib.rs`: Entry point.
*   `src/cert.rs`: Certificate loading and verification logic.
*   `src/gen.rs`: (Optional) Logic to generate temporary identities for bootstrap.
