//! Kubernetes CSR (CertificateSigningRequest) Manager
//!
//! Manages the lifecycle of operational certificates via Kubernetes CSR API:
//! - Generate CSR for the node
//! - Submit to K8s cluster
//! - Auto-approve (if permissions allow)
//! - Retrieve signed certificate
//! - Store for agent use

use base64::{engine::general_purpose, Engine as _};
use k8s_openapi::api::certificates::v1::{
    CertificateSigningRequest, CertificateSigningRequestSpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::{
    api::{Api, PostParams},
    Client,
};
use std::collections::BTreeMap;
use tracing::{info, warn};

const CSR_SIGNER: &str = "kubernetes.io/kube-apiserver-client";
const CSR_USAGES: &[&str] = &["client auth"];

#[allow(dead_code)] // Will be used in agent startup logic
pub struct K8sCsrManager {
    client: Client,
    node_name: String,
}

#[allow(dead_code)] // Will be used in agent startup logic
impl K8sCsrManager {
    /// Create a new K8s CSR manager
    pub async fn new(node_name: String) -> Result<Self, Box<dyn std::error::Error>> {
        let client = Client::try_default().await?;
        Ok(Self { client, node_name })
    }

    /// Generate a CSR for the node and submit to K8s
    /// Returns the certificate PEM if successful
    pub async fn request_certificate(
        &self,
    ) -> Result<(String, String), Box<dyn std::error::Error>> {
        info!("Generating CSR for node: {}", self.node_name);

        // 1. Generate key pair
        let (cert_pem, key_pem) = self.generate_csr_and_key()?;

        // 2. Submit CSR to K8s
        let csr_name = format!("keel-agent-{}", self.node_name);
        self.submit_csr(&csr_name, &cert_pem).await?;

        // 3. Auto-approve if we have permissions (optional)
        if let Err(e) = self.approve_csr(&csr_name).await {
            warn!(
                "Could not auto-approve CSR (may need manual approval): {}",
                e
            );
            info!("Waiting for CSR to be approved by cluster admin...");
        }

        // 4. Wait for and retrieve signed certificate
        let signed_cert = self.wait_for_certificate(&csr_name).await?;

        info!(
            "Successfully obtained K8s-signed certificate for {}",
            self.node_name
        );
        Ok((signed_cert, key_pem))
    }

    /// Generate CSR and private key by leveraging existing keel_crypto functionality
    fn generate_csr_and_key(&self) -> Result<(String, String), Box<dyn std::error::Error>> {
        // For now, generate a self-signed cert and extract the CSR
        // TODO: Update keel_crypto to have a dedicated CSR generation function
        let (cert_pem, key_pem) = keel_crypto::generate_bootstrap_certificate(365 * 24)?;

        // Return the cert as the "CSR" for now
        // In production, we'd want to generate an actual CSR, but for Phase 2 MVP this works
        Ok((cert_pem, key_pem))
    }

    /// Submit CSR to Kubernetes
    async fn submit_csr(
        &self,
        csr_name: &str,
        csr_pem: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let csrs: Api<CertificateSigningRequest> = Api::all(self.client.clone());

        // Create CSR object
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "keel-agent".to_string());
        labels.insert("node".to_string(), self.node_name.clone());

        let csr = CertificateSigningRequest {
            metadata: ObjectMeta {
                name: Some(csr_name.to_string()),
                labels: Some(labels),
                ..Default::default()
            },
            spec: CertificateSigningRequestSpec {
                request: k8s_openapi::ByteString(
                    general_purpose::STANDARD.encode(csr_pem).into_bytes(),
                ),
                signer_name: CSR_SIGNER.to_string(),
                usages: Some(CSR_USAGES.iter().map(|s| s.to_string()).collect()),
                ..Default::default()
            },
            status: None,
        };

        csrs.create(&PostParams::default(), &csr).await?;
        info!("Submitted CSR: {}", csr_name);
        Ok(())
    }

    /// Attempt to auto-approve the CSR (requires cluster-admin or CSR approver role)
    async fn approve_csr(&self, csr_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        use kube::api::PatchParams;

        let csrs: Api<CertificateSigningRequest> = Api::all(self.client.clone());

        // Create approval condition
        let approval_condition =
            k8s_openapi::api::certificates::v1::CertificateSigningRequestCondition {
                type_: "Approved".to_string(),
                status: "True".to_string(),
                reason: Some("AutoApproved".to_string()),
                message: Some("Auto-approved by keel-agent".to_string()),
                last_update_time: Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
                    chrono::Utc::now(),
                )),
                ..Default::default()
            };

        // Patch the status to approve
        let patch = serde_json::json!({
            "status": {
                "conditions": [approval_condition]
            }
        });

        csrs.patch_status(
            csr_name,
            &PatchParams::default(),
            &kube::api::Patch::Merge(patch),
        )
        .await?;
        info!("Auto-approved CSR: {}", csr_name);
        Ok(())
    }

    /// Wait for certificate to be issued and return it
    async fn wait_for_certificate(
        &self,
        csr_name: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let csrs: Api<CertificateSigningRequest> = Api::all(self.client.clone());

        // Poll for certificate (with timeout)
        for attempt in 1..=30 {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            let csr = csrs.get(csr_name).await?;

            if let Some(status) = &csr.status {
                if let Some(cert_bytes) = &status.certificate {
                    let cert_pem = String::from_utf8(cert_bytes.0.clone())?;
                    return Ok(cert_pem);
                }
            }

            if attempt % 5 == 0 {
                info!(
                    "Still waiting for CSR to be signed... (attempt {}/30)",
                    attempt
                );
            }
        }

        Err("Timeout waiting for certificate to be signed".into())
    }

    /// Clean up old CSRs for this node
    pub async fn cleanup_old_csrs(&self) -> Result<(), Box<dyn std::error::Error>> {
        let csrs: Api<CertificateSigningRequest> = Api::all(self.client.clone());

        // List CSRs with our label
        let lp = kube::api::ListParams::default().labels(&format!("node={}", self.node_name));

        let csr_list = csrs.list(&lp).await?;

        for csr in csr_list {
            if let Some(name) = csr.metadata.name {
                info!("Cleaning up old CSR: {}", name);
                if let Err(e) = csrs.delete(&name, &Default::default()).await {
                    warn!("Failed to delete CSR {}: {}", name, e);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csr_constants() {
        assert_eq!(CSR_SIGNER, "kubernetes.io/kube-apiserver-client");
        assert_eq!(CSR_USAGES, &["client auth"]);
    }
}
