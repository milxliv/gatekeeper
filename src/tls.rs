use std::path::Path;

use anyhow::{Context, Result};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair};

/// Ensure a TLS cert/key pair exists at the given paths. If either file is
/// missing, generate a fresh self-signed cert valid for ~10 years covering
/// localhost, 127.0.0.1, and the machine's hostname. Returns true if a cert
/// was generated this call, false if existing files were reused.
pub fn ensure_self_signed(cert_path: &Path, key_path: &Path) -> Result<bool> {
    if cert_path.exists() && key_path.exists() {
        return Ok(false);
    }

    for p in [cert_path, key_path] {
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating dir {}", parent.display()))?;
            }
        }
    }

    let hostname = gethostname::gethostname()
        .into_string()
        .unwrap_or_else(|_| "gatekeeper".to_string());

    let sans = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        hostname.clone(),
    ];

    let mut params =
        CertificateParams::new(sans).context("creating cert params")?;
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, format!("GateKeeper ({hostname})"));
    dn.push(DnType::OrganizationName, "WBBH Engineering");
    params.distinguished_name = dn;

    let key_pair = KeyPair::generate().context("generating key pair")?;
    let cert = params
        .self_signed(&key_pair)
        .context("self-signing cert")?;

    std::fs::write(cert_path, cert.pem())
        .with_context(|| format!("writing {}", cert_path.display()))?;
    std::fs::write(key_path, key_pair.serialize_pem())
        .with_context(|| format!("writing {}", key_path.display()))?;

    Ok(true)
}
