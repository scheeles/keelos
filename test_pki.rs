use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject;
fn main() {
    let _ = CertificateDer::from_pem_slice(b"");
}
