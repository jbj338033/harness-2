use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct TlsMaterials {
    pub cert_chain: Vec<CertificateDer<'static>>,
    pub key: PrivatePkcs8KeyDer<'static>,
    pub fingerprint_hex: String,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl Clone for TlsMaterials {
    fn clone(&self) -> Self {
        Self {
            cert_chain: self.cert_chain.clone(),
            key: self.key.clone_key(),
            fingerprint_hex: self.fingerprint_hex.clone(),
            cert_path: self.cert_path.clone(),
            key_path: self.key_path.clone(),
        }
    }
}

impl TlsMaterials {
    pub fn load_or_create(
        cert_path: &Path,
        key_path: &Path,
        extra_dns: &[String],
    ) -> io::Result<Self> {
        if let Some(parent) = cert_path.parent() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut p = fs::metadata(parent)?.permissions();
                p.set_mode(0o700);
                fs::set_permissions(parent, p)?;
            }
        }

        if cert_path.exists() && key_path.exists() {
            let cert_pem = fs::read_to_string(cert_path)?;
            let key_pem = fs::read_to_string(key_path)?;
            return parse_materials(&cert_pem, &key_pem, cert_path, key_path);
        }

        let (cert_pem, key_pem) = generate_self_signed(extra_dns).map_err(io_err)?;
        fs::write(cert_path, &cert_pem)?;
        fs::write(key_path, &key_pem)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = fs::metadata(key_path)?.permissions();
            p.set_mode(0o600);
            fs::set_permissions(key_path, p)?;
        }
        parse_materials(&cert_pem, &key_pem, cert_path, key_path)
    }

    pub fn server_config(&self) -> Result<rustls::ServerConfig, rustls::Error> {
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();
        let key: PrivateKeyDer<'static> = PrivateKeyDer::Pkcs8(self.key.clone_key());
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(self.cert_chain.clone(), key)
    }
}

fn parse_materials(
    cert_pem: &str,
    key_pem: &str,
    cert_path: &Path,
    key_path: &Path,
) -> io::Result<TlsMaterials> {
    let mut cert_reader = cert_pem.as_bytes();
    let cert_chain: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;
    if cert_chain.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "no certs"));
    }

    let mut key_reader = key_pem.as_bytes();
    let key = rustls_pemfile::pkcs8_private_keys(&mut key_reader)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no pkcs8 key"))?;

    let fingerprint_hex = fingerprint(&cert_chain[0]);
    Ok(TlsMaterials {
        cert_chain,
        key,
        fingerprint_hex,
        cert_path: cert_path.to_path_buf(),
        key_path: key_path.to_path_buf(),
    })
}

fn generate_self_signed(extra_dns: &[String]) -> Result<(String, String), rcgen::Error> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "harness");
    params.distinguished_name = dn;
    params.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into()?),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        SanType::IpAddress(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)),
    ];
    for dns in extra_dns {
        params
            .subject_alt_names
            .push(SanType::DnsName(dns.as_str().try_into()?));
    }
    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    Ok((cert.pem(), key.serialize_pem()))
}

#[must_use]
pub fn fingerprint(cert: &CertificateDer<'_>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(cert.as_ref());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}

fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_or_create_generates_new_materials() {
        let t = TempDir::new().unwrap();
        let cert = t.path().join("tls/cert.pem");
        let key = t.path().join("tls/key.pem");
        let m = TlsMaterials::load_or_create(&cert, &key, &[]).unwrap();
        assert_eq!(m.fingerprint_hex.len(), 64);
        assert!(cert.exists());
        assert!(key.exists());
        assert_eq!(m.cert_chain.len(), 1);
    }

    #[test]
    fn load_or_create_reuses_existing_materials() {
        let t = TempDir::new().unwrap();
        let cert = t.path().join("tls/cert.pem");
        let key = t.path().join("tls/key.pem");
        let a = TlsMaterials::load_or_create(&cert, &key, &[]).unwrap();
        let b = TlsMaterials::load_or_create(&cert, &key, &[]).unwrap();
        assert_eq!(a.fingerprint_hex, b.fingerprint_hex);
    }

    #[test]
    fn server_config_accepts_materials() {
        let t = TempDir::new().unwrap();
        let cert = t.path().join("tls/cert.pem");
        let key = t.path().join("tls/key.pem");
        let m = TlsMaterials::load_or_create(&cert, &key, &[]).unwrap();
        assert!(m.server_config().is_ok());
    }

    #[test]
    fn extra_dns_adds_san() {
        let t = TempDir::new().unwrap();
        let cert = t.path().join("tls/cert.pem");
        let key = t.path().join("tls/key.pem");
        let m = TlsMaterials::load_or_create(&cert, &key, &["harness.example.com".into()]).unwrap();
        assert_eq!(m.fingerprint_hex.len(), 64);
    }

    #[cfg(unix)]
    #[test]
    fn key_permissions_are_600() {
        use std::os::unix::fs::PermissionsExt;
        let t = TempDir::new().unwrap();
        let cert = t.path().join("tls/cert.pem");
        let key = t.path().join("tls/key.pem");
        TlsMaterials::load_or_create(&cert, &key, &[]).unwrap();
        let mode = fs::metadata(&key).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
