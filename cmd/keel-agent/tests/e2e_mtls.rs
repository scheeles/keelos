//! End-to-end RBAC tests over a real mTLS transport.
//!
//! These tests exist because the plaintext harness in `e2e.rs` cannot exercise
//! authorization at all: without TLS there are no peer certificates, so every
//! request takes the development-mode bypass in [`keel_agent::rbac`]. Prior to
//! the fail-closed change, that bypass granted Admin to any caller, and the
//! entire e2e suite ran through it -- so no test could have caught it.
//!
//! This file lives in its own test binary (and therefore its own process)
//! because `rbac::set_tls_enforced` is process-global state. Sharing a process
//! with the plaintext tests in `e2e.rs` would make the two suites fight over it.
//!
//! What is proven here, over a real TLS handshake:
//!   * a client with no certificate is rejected (`UNAUTHENTICATED`)
//!   * a `keel:viewer` certificate cannot reach an Admin-only RPC
//!   * a `keel:viewer` certificate can reach a Viewer RPC
//!   * a `keel:admin` certificate can reach both

use keel_api::node::node_service_client::NodeServiceClient;
use keel_api::node::node_service_server::NodeServiceServer;
use keel_api::node::{GetStatusRequest, RebootRequest};
use std::net::SocketAddr;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity, Server, ServerTlsConfig};

/// The DNS name the server certificate is issued for.
const SERVER_DNS_NAME: &str = "keel-node.test";

/// A minimal CA plus the leaf certificates issued from it.
struct TestPki {
    ca_pem: String,
    server_cert_pem: String,
    server_key_pem: String,
}

impl TestPki {
    /// Build a CA and a server certificate valid for [`SERVER_DNS_NAME`].
    fn new() -> (Self, CaSigner) {
        use rcgen::{
            BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair,
            KeyUsagePurpose,
        };

        let ca_key = KeyPair::generate().expect("CA key generation");
        let mut ca_params = CertificateParams::new(Vec::new()).expect("CA params");
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "keel-test-ca");
        ca_params.distinguished_name = ca_dn;

        let ca_cert = ca_params.self_signed(&ca_key).expect("CA self-sign");
        let ca_pem = ca_cert.pem();

        // Server leaf, signed by the CA.
        let server_key = KeyPair::generate().expect("server key generation");
        let mut server_params =
            CertificateParams::new(vec![SERVER_DNS_NAME.to_string()]).expect("server params");
        let mut server_dn = DistinguishedName::new();
        server_dn.push(DnType::CommonName, SERVER_DNS_NAME);
        server_params.distinguished_name = server_dn;

        let issuer = Issuer::new(ca_params, ca_key);
        let server_cert = server_params
            .signed_by(&server_key, &issuer)
            .expect("server cert signing");

        let pki = Self {
            ca_pem,
            server_cert_pem: server_cert.pem(),
            server_key_pem: server_key.serialize_pem(),
        };

        (pki, CaSigner { issuer })
    }
}

/// Issues client certificates from the test CA.
struct CaSigner {
    issuer: rcgen::Issuer<'static, rcgen::KeyPair>,
}

impl CaSigner {
    /// Issue a client certificate whose Organization field carries `org`.
    ///
    /// `keel_agent::rbac` maps this field to a role, following the Kubernetes
    /// convention where the certificate O field denotes group membership.
    fn issue_client(&self, org: &str) -> (String, String) {
        use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

        let key = KeyPair::generate().expect("client key generation");
        let mut params = CertificateParams::new(Vec::new()).expect("client params");
        let mut dn = DistinguishedName::new();
        dn.push(DnType::OrganizationName, org);
        dn.push(DnType::CommonName, "keel-test-client");
        params.distinguished_name = dn;

        let cert = params
            .signed_by(&key, &self.issuer)
            .expect("client cert signing");

        (cert.pem(), key.serialize_pem())
    }
}

/// Start an mTLS gRPC server on an ephemeral port.
///
/// Mirrors the production configuration in `mtls.rs`, including
/// `client_auth_optional(true)` -- that setting is deliberate so the
/// unauthenticated `InitBootstrap` enrolment endpoint stays reachable, and it is
/// precisely why RBAC (not the TLS layer) has to reject certless callers.
async fn start_mtls_server(pki: &TestPki) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let scheduler = std::sync::Arc::new(keel_agent::UpdateScheduler::new(format!(
        "/tmp/keel-e2e-mtls-{}.json",
        addr.port()
    )));
    let health_checker = std::sync::Arc::new(keel_agent::HealthChecker::new(
        keel_agent::HealthCheckerConfig::default(),
    ));
    let diagnostics = std::sync::Arc::new(keel_agent::DiagnosticsManager::new());

    let service = keel_agent::HelperNodeService {
        scheduler,
        health_checker,
        diagnostics,
    };

    let tls = ServerTlsConfig::new()
        .identity(Identity::from_pem(
            &pki.server_cert_pem,
            &pki.server_key_pem,
        ))
        .client_ca_root(Certificate::from_pem(&pki.ca_pem))
        .client_auth_optional(true);

    tokio::spawn(async move {
        Server::builder()
            .tls_config(tls)
            .expect("server TLS config")
            .add_service(NodeServiceServer::new(service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    Ok(addr)
}

/// Connect a client, optionally presenting a client certificate.
async fn connect(
    addr: SocketAddr,
    ca_pem: &str,
    client_identity: Option<(String, String)>,
) -> Result<NodeServiceClient<Channel>, Box<dyn std::error::Error>> {
    let mut tls = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_pem))
        .domain_name(SERVER_DNS_NAME);

    if let Some((cert_pem, key_pem)) = client_identity {
        tls = tls.identity(Identity::from_pem(cert_pem, key_pem));
    }

    let channel = Channel::from_shared(format!("https://{addr}"))?
        .tls_config(tls)?
        .connect()
        .await?;

    Ok(NodeServiceClient::new(channel))
}

/// Prepare this test process: install the rustls CryptoProvider and enable RBAC
/// enforcement. Both are process-global and must happen exactly once.
fn init_test_process() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Both aws-lc-rs and ring are in the dependency graph, so rustls cannot
        // pick a default on its own. Must match the provider installed by the
        // agent in main().
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        keel_agent::rbac::set_tls_enforced(true);
    });
}

#[tokio::test]
async fn client_without_certificate_is_rejected() {
    init_test_process();
    let (pki, _ca) = TestPki::new();
    let addr = start_mtls_server(&pki).await.expect("server start");

    let mut client = connect(addr, &pki.ca_pem, None)
        .await
        .expect("TLS handshake should succeed without a client certificate");

    // GetStatus only requires Viewer -- the lowest privilege level. Even that
    // must be refused when the caller is anonymous.
    let status = client
        .get_status(GetStatusRequest {})
        .await
        .expect_err("anonymous caller must be rejected");

    assert_eq!(
        status.code(),
        tonic::Code::Unauthenticated,
        "expected UNAUTHENTICATED, got: {status:?}"
    );
}

#[tokio::test]
async fn viewer_certificate_cannot_reach_admin_endpoint() {
    init_test_process();
    let (pki, ca) = TestPki::new();
    let addr = start_mtls_server(&pki).await.expect("server start");

    let mut client = connect(addr, &pki.ca_pem, Some(ca.issue_client("keel:viewer")))
        .await
        .expect("client connect");

    // Reboot requires Admin.
    let status = client
        .reboot(RebootRequest {
            reason: "test".to_string(),
        })
        .await
        .expect_err("viewer must not be able to reboot the node");

    assert_eq!(
        status.code(),
        tonic::Code::PermissionDenied,
        "expected PERMISSION_DENIED, got: {status:?}"
    );
}

#[tokio::test]
async fn viewer_certificate_can_reach_viewer_endpoint() {
    init_test_process();
    let (pki, ca) = TestPki::new();
    let addr = start_mtls_server(&pki).await.expect("server start");

    let mut client = connect(addr, &pki.ca_pem, Some(ca.issue_client("keel:viewer")))
        .await
        .expect("client connect");

    client
        .get_status(GetStatusRequest {})
        .await
        .expect("viewer must be able to read status");
}

#[tokio::test]
async fn admin_certificate_can_reach_admin_endpoint() {
    init_test_process();
    let (pki, ca) = TestPki::new();
    let addr = start_mtls_server(&pki).await.expect("server start");

    let mut client = connect(addr, &pki.ca_pem, Some(ca.issue_client("keel:admin")))
        .await
        .expect("client connect");

    client
        .reboot(RebootRequest {
            reason: "e2e mtls test".to_string(),
        })
        .await
        .expect("admin must be able to reboot the node");

    client
        .get_status(GetStatusRequest {})
        .await
        .expect("admin must be able to read status");
}

#[tokio::test]
async fn certificate_from_untrusted_ca_is_rejected_at_tls_layer() {
    init_test_process();
    let (pki, _ca) = TestPki::new();
    let addr = start_mtls_server(&pki).await.expect("server start");

    // A second, unrelated CA issues a certificate claiming the admin role.
    let (_rogue_pki, rogue_ca) = TestPki::new();
    let rogue_identity = rogue_ca.issue_client("keel:admin");

    let result = connect(addr, &pki.ca_pem, Some(rogue_identity)).await;

    // The handshake itself may fail, or it may succeed and the first RPC be
    // refused; either is acceptable. What must not happen is a successful
    // privileged call.
    if let Ok(mut client) = result {
        let status = client
            .reboot(RebootRequest {
                reason: "rogue".to_string(),
            })
            .await
            .expect_err("certificate from an untrusted CA must not be honoured");
        assert!(
            matches!(
                status.code(),
                tonic::Code::Unauthenticated | tonic::Code::PermissionDenied | tonic::Code::Unknown
            ),
            "unexpected status for rogue CA: {status:?}"
        );
    }
}
