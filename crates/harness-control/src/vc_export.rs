// IMPLEMENTS: D-314
//! Consent Ledger → W3C Verifiable Credentials v2 export. Takes the
//! D-187 principal_id record and lays it into a VC envelope so a
//! third-party wallet can verify the consent independently. The
//! `proof` field is left to the signer (D-343 ed25519 layer).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcCredentialSubject {
    pub id: String,
    pub principal_id: String,
    pub scope: String,
    pub granted_at_iso: String,
    pub revoked_at_iso: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VcExport {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    #[serde(rename = "type")]
    pub type_: Vec<String>,
    pub issuer: String,
    pub valid_from: String,
    pub valid_until: Option<String>,
    pub credential_subject: VcCredentialSubject,
}

#[must_use]
pub fn export_consent_to_vc(issuer: impl Into<String>, subject: VcCredentialSubject) -> VcExport {
    VcExport {
        context: vec![
            "https://www.w3.org/ns/credentials/v2".into(),
            "https://harness.local/credentials/consent/v1".into(),
        ],
        type_: vec![
            "VerifiableCredential".into(),
            "HarnessConsentCredential".into(),
        ],
        issuer: issuer.into(),
        valid_from: subject.granted_at_iso.clone(),
        valid_until: subject.revoked_at_iso.clone(),
        credential_subject: subject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject() -> VcCredentialSubject {
        VcCredentialSubject {
            id: "did:harness:user-1".into(),
            principal_id: "p-1".into(),
            scope: "tools.shell.read".into(),
            granted_at_iso: "2026-04-22T10:00:00Z".into(),
            revoked_at_iso: None,
        }
    }

    #[test]
    fn vc_envelope_has_credentials_v2_context() {
        let v = export_consent_to_vc("did:harness:daemon", subject());
        assert!(
            v.context
                .contains(&"https://www.w3.org/ns/credentials/v2".into())
        );
        assert!(v.type_.contains(&"VerifiableCredential".into()));
    }

    #[test]
    fn valid_until_round_trips_when_revoked() {
        let mut s = subject();
        s.revoked_at_iso = Some("2026-05-22T10:00:00Z".into());
        let v = export_consent_to_vc("issuer", s);
        assert_eq!(v.valid_until.as_deref(), Some("2026-05-22T10:00:00Z"));
    }

    #[test]
    fn export_round_trips_via_serde() {
        let v = export_consent_to_vc("issuer", subject());
        let s = serde_json::to_string(&v).unwrap();
        let back: VcExport = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
        assert!(s.contains("@context"));
    }
}
