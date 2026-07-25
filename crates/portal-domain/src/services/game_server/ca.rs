//! Portal-private certificate authority for server agents.
//!
//! The portal signs agent client certificates during enrollment; Caddy
//! verifies chain + validity against the CA cert, and the API checks the
//! presented serial against `server_agent_certs` for revocation and server
//! binding. Design: docs/matchzy-integration.md §5.3–§5.4.
//!
//! Deliberately sync (rcgen is pure computation); callers on async paths
//! call it inline — signing is sub-millisecond.

use chrono::{DateTime, Utc};
use portal_core::errors::DomainError;
use rand::Rng;
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DistinguishedName,
    DnType, IsCa, KeyPair, SerialNumber,
};
use sha2::{Digest, Sha256};

/// Default validity for issued agent certificates (days).
pub const AGENT_CERT_VALIDITY_DAYS: i64 = 90;

/// A certificate issued for an agent CSR.
#[derive(Debug, Clone)]
pub struct IssuedCertificate {
    /// PEM-encoded signed certificate.
    pub cert_pem: String,
    /// Hex-encoded serial number (matches what TLS peers present).
    pub serial: String,
    /// SHA-256 fingerprint of the DER certificate, hex-encoded.
    pub fingerprint_sha256: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
}

/// Freshly generated CA material for `portal-cli gameserver ca-init`.
#[derive(Debug, Clone)]
pub struct GeneratedCa {
    pub cert_pem: String,
    pub key_pem: String,
}

/// The loaded portal CA, able to sign agent CSRs.
pub struct CertificateAuthority {
    cert_pem: String,
    issuer: rcgen::Certificate,
    key: KeyPair,
}

impl CertificateAuthority {
    /// Generate a new self-signed CA (10-year validity).
    pub fn generate(common_name: &str) -> Result<GeneratedCa, DomainError> {
        let key = KeyPair::generate().map_err(ca_err)?;
        let mut params = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, common_name);
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + time::Duration::days(3650);
        params.serial_number = Some(SerialNumber::from(random_serial().to_vec()));
        let cert = params.self_signed(&key).map_err(ca_err)?;
        Ok(GeneratedCa {
            cert_pem: cert.pem(),
            key_pem: key.serialize_pem(),
        })
    }

    /// Load the CA from PEM material (as written by `ca-init`).
    pub fn from_pem(cert_pem: &str, key_pem: &str) -> Result<Self, DomainError> {
        let key = KeyPair::from_pem(key_pem).map_err(ca_err)?;
        let params = CertificateParams::from_ca_cert_pem(cert_pem).map_err(ca_err)?;
        // Re-signing with the same key reproduces a Certificate usable as an
        // issuer; the original PEM (with its original signature) is what we
        // hand out as the trust anchor.
        let issuer = params.self_signed(&key).map_err(ca_err)?;
        Ok(Self {
            cert_pem: cert_pem.to_string(),
            issuer,
            key,
        })
    }

    /// The CA certificate PEM (the trust anchor agents and Caddy pin).
    #[must_use]
    pub fn cert_pem(&self) -> &str {
        &self.cert_pem
    }

    /// Sign an agent CSR, binding the certificate to `common_name`
    /// (the server's UUID — the requested DN is ignored on purpose).
    pub fn sign_csr(
        &self,
        csr_pem: &str,
        common_name: &str,
        validity_days: i64,
    ) -> Result<IssuedCertificate, DomainError> {
        let mut csr = CertificateSigningRequestParams::from_pem(csr_pem)
            .map_err(|e| DomainError::InvalidState(format!("invalid CSR: {e}")))?;

        // Identity is portal-assigned, never requested.
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, common_name);
        csr.params.distinguished_name = dn;
        csr.params.is_ca = IsCa::NoCa;

        let now = time::OffsetDateTime::now_utc();
        csr.params.not_before = now;
        csr.params.not_after = now + time::Duration::days(validity_days);

        let serial_bytes = random_serial();
        csr.params.serial_number = Some(SerialNumber::from(serial_bytes.to_vec()));

        let cert = csr.signed_by(&self.issuer, &self.key).map_err(ca_err)?;

        let fingerprint = hex::encode(Sha256::digest(cert.der()));
        Ok(IssuedCertificate {
            cert_pem: cert.pem(),
            serial: hex::encode(serial_bytes),
            fingerprint_sha256: fingerprint,
            not_before: odt_to_chrono(now),
            not_after: odt_to_chrono(now + time::Duration::days(validity_days)),
        })
    }
}

/// 16 random bytes with the high bit clear (positive DER integer).
fn random_serial() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    bytes[0] &= 0x7f;
    bytes
}

fn odt_to_chrono(odt: time::OffsetDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(odt.unix_timestamp(), 0)
        .expect("valid unix timestamp from OffsetDateTime")
}

fn ca_err(e: rcgen::Error) -> DomainError {
    DomainError::Internal(format!("certificate authority error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_csr() -> (String, KeyPair) {
        let key = KeyPair::generate().unwrap();
        let params = CertificateParams::default();
        let csr = params.serialize_request(&key).unwrap();
        (csr.pem().unwrap(), key)
    }

    #[test]
    fn ca_signs_csr_with_assigned_identity() {
        let ca_material = CertificateAuthority::generate("portal-agent-ca").unwrap();
        let ca =
            CertificateAuthority::from_pem(&ca_material.cert_pem, &ca_material.key_pem).unwrap();

        let (csr_pem, _key) = make_csr();
        let issued = ca
            .sign_csr(&csr_pem, "0198f00d-0000-7000-8000-000000000001", 90)
            .unwrap();

        assert!(issued.cert_pem.contains("BEGIN CERTIFICATE"));
        assert_eq!(issued.serial.len(), 32); // 16 bytes hex
        assert_eq!(issued.fingerprint_sha256.len(), 64);
        assert!(issued.not_after > issued.not_before);
    }

    #[test]
    fn invalid_csr_is_rejected() {
        let ca_material = CertificateAuthority::generate("portal-agent-ca").unwrap();
        let ca =
            CertificateAuthority::from_pem(&ca_material.cert_pem, &ca_material.key_pem).unwrap();
        assert!(ca.sign_csr("not a csr", "cn", 90).is_err());
    }
}
