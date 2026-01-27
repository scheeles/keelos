use anyhow::{anyhow, Result};
use std::fs;
use std::path::PathBuf;
use tracing::info;

/// Rotate client certificate by requesting a new one from Kubernetes
pub async fn rotate_certificate(
    kubeconfig_path: &str,
    cert_dir: Option<PathBuf>,
    cert_name: Option<&str>,
    auto_approve: bool,
) -> Result<()> {
    info!("Starting certificate rotation...");

    // Determine certificate directory
    let cert_path = if let Some(dir) = cert_dir {
        dir
    } else {
        dirs::home_dir()
            .ok_or_else(|| anyhow!("Could not determine home directory"))?
            .join(".keel")
    };

    let cert_file = cert_path.join("client.pem");
    let key_file = cert_path.join("client.key");
    let _ca_file = cert_path.join("ca.pem");

    // Check if existing certificate exists
    if !cert_file.exists() {
        return Err(anyhow!(
            "No existing certificate found at: {}",
            cert_file.display()
        ));
    }

    // Check expiry of existing certificate
    info!("Checking existing certificate expiry...");
    match keel_crypto::cert_expiry::days_until_expiry(&cert_file) {
        Ok(days) => {
            info!("Current certificate expires in {} days", days);
            if days > 30 {
                println!("ℹ️  Certificate still valid for {} days", days);
                println!("   Rotation not required yet, but proceeding anyway...");
            }
        }
        Err(e) => {
            println!("⚠️  Could not check certificate expiry: {}", e);
        }
    }

    // Backup existing certificates
    info!("Backing up existing certificates...");
    fs::copy(&cert_file, cert_path.join("client.pem.old"))?;
    fs::copy(&key_file, cert_path.join("client.key.old"))?;
    println!("✓ Backed up existing certificates");

    // Request new certificate using same logic as init
    println!("\n🔄 Requesting new certificate from Kubernetes...");
    super::init::init_kubernetes(
        kubeconfig_path,
        cert_path,
        cert_name.unwrap_or("keel-node"),
        auto_approve,
    )
    .await?;

    println!("\n✅ Certificate rotation complete!");
    println!("   Old certificates backed up with .old extension");

    Ok(())
}

/// Show certificate information including expiry
pub fn show_cert_info(cert_path: Option<PathBuf>) -> Result<()> {
    let cert_dir = if let Some(dir) = cert_path {
        dir
    } else {
        dirs::home_dir()
            .ok_or_else(|| anyhow!("Could not determine home directory"))?
            .join(".keel")
    };

    let cert_file = cert_dir.join("client.pem");

    if !cert_file.exists() {
        println!("❌ No certificate found at: {}", cert_file.display());
        return Ok(());
    }

    println!("📜 Certificate Information\n");
    println!("Path: {}", cert_file.display());

    // Get subject
    match keel_crypto::cert_expiry::get_cert_subject_cn(&cert_file) {
        Ok(cn) => println!("Subject: CN={}", cn),
        Err(e) => println!("Subject: Error reading ({})", e),
    }

    // Get expiry
    match keel_crypto::cert_expiry::days_until_expiry(&cert_file) {
        Ok(days) if days < 0 => {
            println!("\n❌ Status: EXPIRED ({} days ago)", days.abs());
            println!("   Action: Run 'osctl rotate-cert' immediately");
        }
        Ok(days) if days < 7 => {
            println!("\n⚠️  Status: Expires in {} days", days);
            println!("   Action: Rotate soon with 'osctl rotate-cert'");
        }
        Ok(days) if days < 30 => {
            println!("\n⚡ Status: Expires in {} days", days);
            println!("   Action: Consider rotating with 'osctl rotate-cert'");
        }
        Ok(days) => {
            println!("\n✅ Status: Valid ({} days remaining)", days);
        }
        Err(e) => {
            println!("\n❌ Status: Error checking expiry ({})", e);
        }
    }

    Ok(())
}
