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

use tonic::{Request, Status};
use tracing::{debug, warn};

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

/// Authorize a gRPC request against the required role.
///
/// Extracts the client certificate from the TLS peer connection info
/// and checks that the certificate's role has sufficient privilege.
///
/// When TLS is not configured (development mode), all requests are allowed.
///
/// # Errors
///
/// Returns `Status::unauthenticated` if no valid client certificate is present.
/// Returns `Status::permission_denied` if the client's role is insufficient.
pub fn authorize<T>(request: &Request<T>, required: Role) -> Result<(), Status> {
    let Some(peer_certs) = request.peer_certs() else {
        // No TLS peer certs — either TLS is not configured (dev mode)
        // or client connected without a certificate.
        // When mTLS is properly configured, tonic validates the cert chain;
        // absence of peer_certs here means TLS is disabled entirely.
        debug!("No peer certificates found — allowing request (TLS may be disabled)");
        return Ok(());
    };

    let first_cert = peer_certs
        .first()
        .ok_or_else(|| Status::unauthenticated("Client certificate chain is empty"))?;

    let client_role = role_from_cert_der(first_cert.as_ref()).map_err(|e| {
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

    // --- authorize() tests (without TLS — dev mode) ---

    #[test]
    fn test_authorize_no_tls_allows_all() {
        // Without TLS, peer_certs() returns None, so all requests are allowed
        let request = Request::new(());
        assert!(authorize(&request, Role::Admin).is_ok());
        assert!(authorize(&request, Role::Operator).is_ok());
        assert!(authorize(&request, Role::Viewer).is_ok());
    }
}
