//! Role-based access control (RBAC) for gRPC endpoints
//!
//! Maps client certificate identity to roles and enforces per-endpoint
//! authorization based on Kubernetes RBAC conventions.
//!
//! # Role Hierarchy
//!
//! - **Admin**: Full access including dangerous operations (reboot, rollback, network config)
//! - **Operator**: Operational tasks (updates, snapshots, log streaming, cert rotation)
//! - **Viewer**: Read-only access (status, health, schedules, history)
//!
//! # Certificate Identity
//!
//! Roles are extracted from the client certificate's Organization (O) field,
//! following Kubernetes conventions where the O field maps to groups:
//!
//! - `system:masters` or `keel:admin` → Admin
//! - `keel:operator` → Operator
//! - `keel:viewer` → Viewer

use std::sync::atomic::{AtomicBool, Ordering};
use tonic::{Request, Status};
use tracing::{debug, info, warn};

/// RBAC roles ordered by privilege level (lowest to highest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Read-only access to status and health endpoints
    Viewer,
    /// Operational tasks (updates, snapshots, cert rotation)
    Operator,
    /// Full access including dangerous operations
    Admin,
}

impl Role {
    /// Returns the privilege level for comparison (higher = more privileged).
    fn privilege_level(self) -> u8 {
        match self {
            Self::Viewer => 0,
            Self::Operator => 1,
            Self::Admin => 2,
        }
    }

    /// Check if this role has sufficient privilege for the required role.
    pub fn has_permission(self, required: Self) -> bool {
        self.privilege_level() >= required.privilege_level()
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Viewer => write!(f, "viewer"),
            Self::Operator => write!(f, "operator"),
            Self::Admin => write!(f, "admin"),
        }
    }
}

/// Extract the RBAC role from a DER-encoded X.509 client certificate.
///
/// Checks the Organization (O) field of the certificate subject for
/// recognized role identifiers following Kubernetes conventions.
///
/// # Errors
///
/// Returns an error if the certificate cannot be parsed or contains
/// no recognized role in its Organization field.
pub fn role_from_cert_der(cert_der: &[u8]) -> Result<Role, String> {
    use x509_parser::prelude::*;

    let (_, cert) = X509Certificate::from_der(cert_der)
        .map_err(|e| format!("Failed to parse X.509 certificate: {e}"))?;

    // Check Organization (O) field — standard for K8s group mapping
    for attr in cert.subject().iter_organization() {
        if let Ok(val) = attr.as_str() {
            match val {
                "system:masters" | "keel:admin" => return Ok(Role::Admin),
                "keel:operator" => return Ok(Role::Operator),
                "keel:viewer" => return Ok(Role::Viewer),
                _ => {}
            }
        }
    }

    Err("No recognized RBAC role in certificate Organization field".to_string())
}

/// Whether the gRPC server was started with client-certificate verification
/// configured.
///
/// `Request::peer_certs()` returns `None` both when the server is running in
/// plaintext development mode *and* when a client connects over TLS without
/// presenting a certificate. Those two cases require opposite decisions, and
/// they are indistinguishable from the request alone, so the server records its
/// own TLS state here at startup.
static TLS_ENFORCED: AtomicBool = AtomicBool::new(false);

/// Record whether the server is enforcing client certificates.
///
/// Called once by `main` after the TLS configuration outcome is known. When
/// this is `false`, unauthenticated requests are permitted so the agent remains
/// usable in local development and in the QEMU test harness.
pub fn set_tls_enforced(enforced: bool) {
    TLS_ENFORCED.store(enforced, Ordering::Relaxed);
    if enforced {
        info!("RBAC: client certificates required for all authorized endpoints");
    } else {
        warn!(
            "RBAC: TLS is not configured - authorization is DISABLED and every \
             endpoint is reachable anonymously. Development use only."
        );
    }
}

/// Whether client certificates are currently required.
pub fn is_tls_enforced() -> bool {
    TLS_ENFORCED.load(Ordering::Relaxed)
}

/// The client certificate material associated with a request.
///
/// Modelled explicitly so the authorization decision can be unit tested for
/// every case without standing up a TLS connection.
#[derive(Debug, Clone, Copy)]
enum PeerIdentity<'a> {
    /// No TLS peer information at all — a plaintext connection.
    NoTls,
    /// A TLS connection whose client certificate chain was empty.
    EmptyChain,
    /// A TLS connection presenting a leaf certificate (DER-encoded).
    Leaf(&'a [u8]),
}

/// Decide whether a request is authorized.
///
/// Pure function: no global state, no I/O. `authorize` is a thin wrapper that
/// supplies `tls_enforced` and the peer certificate from the live request.
fn authorize_decision(
    peer: PeerIdentity<'_>,
    tls_enforced: bool,
    required: Role,
) -> Result<(), Status> {
    let leaf = match peer {
        PeerIdentity::Leaf(der) => der,
        PeerIdentity::NoTls | PeerIdentity::EmptyChain => {
            if tls_enforced {
                // The server is configured for mTLS and the caller presented no
                // certificate. Deny.
                //
                // This previously returned Ok(()), which meant any peer that
                // simply omitted its client certificate was granted Admin --
                // the server binds 0.0.0.0:50051 and sets client_auth_optional
                // so that InitBootstrap can be reached during enrolment, so
                // certless connections are accepted at the TLS layer by design.
                warn!(
                    required_role = %required,
                    "RBAC: denying request with no client certificate"
                );
                return Err(Status::unauthenticated(
                    "a client certificate is required for this operation",
                ));
            }
            debug!(
                required_role = %required,
                "RBAC: TLS not configured - allowing unauthenticated request (development mode)"
            );
            return Ok(());
        }
    };

    let client_role = role_from_cert_der(leaf).map_err(|e| {
        warn!(error = %e, "RBAC: failed to extract role from client certificate");
        Status::permission_denied(format!("Unrecognized client certificate role: {e}"))
    })?;

    if client_role.has_permission(required) {
        debug!(
            client_role = %client_role,
            required_role = %required,
            "RBAC: access granted"
        );
        Ok(())
    } else {
        warn!(
            client_role = %client_role,
            required_role = %required,
            "RBAC: access denied — insufficient permissions"
        );
        Err(Status::permission_denied(format!(
            "Role '{client_role}' does not have permission for this operation (requires '{required}')"
        )))
    }
}

/// Authorize a gRPC request against the required role.
///
/// Extracts the client certificate from the TLS peer connection info
/// and checks that the certificate's role has sufficient privilege.
///
/// When the server is not running with TLS, all requests are allowed so the
/// agent stays usable in development. When TLS *is* configured, a request
/// without a client certificate is rejected.
///
/// # Errors
///
/// Returns `Status::unauthenticated` if no valid client certificate is present.
/// Returns `Status::permission_denied` if the client's role is insufficient.
pub fn authorize<T>(request: &Request<T>, required: Role) -> Result<(), Status> {
    let peer_certs = request.peer_certs();
    let peer = match peer_certs.as_deref().map(Vec::as_slice) {
        None => PeerIdentity::NoTls,
        Some([]) => PeerIdentity::EmptyChain,
        Some([leaf, ..]) => PeerIdentity::Leaf(leaf.as_ref()),
    };

    authorize_decision(peer, is_tls_enforced(), required)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Role tests ---

    #[test]
    fn test_role_privilege_ordering() {
        assert!(Role::Admin.has_permission(Role::Admin));
        assert!(Role::Admin.has_permission(Role::Operator));
        assert!(Role::Admin.has_permission(Role::Viewer));

        assert!(!Role::Operator.has_permission(Role::Admin));
        assert!(Role::Operator.has_permission(Role::Operator));
        assert!(Role::Operator.has_permission(Role::Viewer));

        assert!(!Role::Viewer.has_permission(Role::Admin));
        assert!(!Role::Viewer.has_permission(Role::Operator));
        assert!(Role::Viewer.has_permission(Role::Viewer));
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::Admin.to_string(), "admin");
        assert_eq!(Role::Operator.to_string(), "operator");
        assert_eq!(Role::Viewer.to_string(), "viewer");
    }

    // --- Certificate role extraction tests ---

    /// Helper to generate a self-signed certificate with given Organization field.
    fn generate_cert_with_org(org: &str) -> Vec<u8> {
        use rcgen::{CertificateParams, DistinguishedName, KeyPair};

        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(rcgen::DnType::OrganizationName, org);
        dn.push(rcgen::DnType::CommonName, "test-user");
        params.distinguished_name = dn;

        let key_pair = KeyPair::generate().expect("key generation should succeed");
        let cert = params
            .self_signed(&key_pair)
            .expect("cert generation should succeed");
        cert.der().to_vec()
    }

    #[test]
    fn test_role_from_cert_admin_system_masters() {
        let cert_der = generate_cert_with_org("system:masters");
        let role = role_from_cert_der(&cert_der);
        assert!(role.is_ok());
        assert_eq!(role.unwrap(), Role::Admin);
    }

    #[test]
    fn test_role_from_cert_admin_keel() {
        let cert_der = generate_cert_with_org("keel:admin");
        let role = role_from_cert_der(&cert_der);
        assert!(role.is_ok());
        assert_eq!(role.unwrap(), Role::Admin);
    }

    #[test]
    fn test_role_from_cert_operator() {
        let cert_der = generate_cert_with_org("keel:operator");
        let role = role_from_cert_der(&cert_der);
        assert!(role.is_ok());
        assert_eq!(role.unwrap(), Role::Operator);
    }

    #[test]
    fn test_role_from_cert_viewer() {
        let cert_der = generate_cert_with_org("keel:viewer");
        let role = role_from_cert_der(&cert_der);
        assert!(role.is_ok());
        assert_eq!(role.unwrap(), Role::Viewer);
    }

    #[test]
    fn test_role_from_cert_unknown_org() {
        let cert_der = generate_cert_with_org("unknown-org");
        let role = role_from_cert_der(&cert_der);
        assert!(role.is_err());
        assert!(role.unwrap_err().contains("No recognized RBAC role"));
    }

    #[test]
    fn test_role_from_cert_invalid_der() {
        let role = role_from_cert_der(b"not a certificate");
        assert!(role.is_err());
        assert!(role.unwrap_err().contains("Failed to parse"));
    }

    // --- authorize_decision() tests ---
    //
    // These exercise the pure decision function directly, which covers every
    // combination of peer identity and TLS mode without needing a live TLS
    // connection. The e2e suite additionally proves the same rules hold over a
    // real mTLS transport.

    #[test]
    fn test_no_client_cert_is_denied_when_tls_enforced() {
        // The regression this guards: `authorize` used to return Ok(()) here,
        // so any peer that omitted its client certificate was granted Admin.
        for required in [Role::Admin, Role::Operator, Role::Viewer] {
            let err = authorize_decision(PeerIdentity::NoTls, true, required)
                .expect_err("certless request must be denied when TLS is enforced");
            assert_eq!(err.code(), tonic::Code::Unauthenticated);

            let err = authorize_decision(PeerIdentity::EmptyChain, true, required)
                .expect_err("empty chain must be denied when TLS is enforced");
            assert_eq!(err.code(), tonic::Code::Unauthenticated);
        }
    }

    #[test]
    fn test_no_client_cert_is_allowed_when_tls_not_configured() {
        // Development mode: the agent must stay usable without certificates,
        // including in the QEMU test harness.
        for required in [Role::Admin, Role::Operator, Role::Viewer] {
            assert!(authorize_decision(PeerIdentity::NoTls, false, required).is_ok());
        }
    }

    #[test]
    fn test_admin_cert_satisfies_every_role() {
        let der = generate_cert_with_org("keel:admin");
        for required in [Role::Admin, Role::Operator, Role::Viewer] {
            assert!(authorize_decision(PeerIdentity::Leaf(&der), true, required).is_ok());
        }
    }

    #[test]
    fn test_viewer_cert_is_denied_operator_and_admin_operations() {
        let der = generate_cert_with_org("keel:viewer");

        assert!(authorize_decision(PeerIdentity::Leaf(&der), true, Role::Viewer).is_ok());

        for required in [Role::Operator, Role::Admin] {
            let err = authorize_decision(PeerIdentity::Leaf(&der), true, required)
                .expect_err("viewer must not be granted a higher-privileged operation");
            assert_eq!(err.code(), tonic::Code::PermissionDenied);
        }
    }

    #[test]
    fn test_operator_cert_is_denied_admin_operations() {
        let der = generate_cert_with_org("keel:operator");

        assert!(authorize_decision(PeerIdentity::Leaf(&der), true, Role::Operator).is_ok());
        assert!(authorize_decision(PeerIdentity::Leaf(&der), true, Role::Viewer).is_ok());

        let err = authorize_decision(PeerIdentity::Leaf(&der), true, Role::Admin)
            .expect_err("operator must not be granted an admin operation");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn test_unrecognized_org_is_denied_even_with_valid_cert() {
        let der = generate_cert_with_org("some-other-org");
        let err = authorize_decision(PeerIdentity::Leaf(&der), true, Role::Viewer)
            .expect_err("a certificate with no known role must be denied");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn test_role_checks_still_apply_when_tls_not_enforced_but_cert_present() {
        // A presented certificate is always evaluated on its merits; the
        // development-mode bypass only covers the *absence* of a certificate.
        let der = generate_cert_with_org("keel:viewer");
        let err = authorize_decision(PeerIdentity::Leaf(&der), false, Role::Admin)
            .expect_err("a viewer cert must not reach an admin operation");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn test_tls_enforced_flag_roundtrip() {
        // Default is fail-open so that a server which never calls
        // set_tls_enforced (tests, dev) keeps working.
        set_tls_enforced(true);
        assert!(is_tls_enforced());
        set_tls_enforced(false);
        assert!(!is_tls_enforced());
    }
}
