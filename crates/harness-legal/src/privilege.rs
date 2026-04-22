// IMPLEMENTS: D-356
//! Client-confidential taint + provider manifest. Any value derived
//! from privileged input carries [`PrivilegeTaint`]; only providers
//! listed in the matter's [`PrivilegeManifest`] may receive it. The
//! taint engine itself is shared with `harness-taint` (D-350) — this
//! module is the legal-mode façade with stronger defaults.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegeTaint {
    /// Default for legal-mode strings.
    #[default]
    ClientConfidential,
    /// Public legal data — citations, statutes — may move freely.
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivilegeManifest {
    pub allowed: BTreeSet<ProviderId>,
}

impl PrivilegeManifest {
    pub fn new<I: IntoIterator<Item = ProviderId>>(allowed: I) -> Self {
        Self {
            allowed: allowed.into_iter().collect(),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PrivilegeError {
    #[error(
        "provider {0:?} is not on the privilege manifest — refused to send client-confidential bytes"
    )]
    ProviderNotAllowed(ProviderId),
}

pub fn check_send(
    manifest: &PrivilegeManifest,
    provider: &ProviderId,
    taint: PrivilegeTaint,
) -> Result<(), PrivilegeError> {
    if matches!(taint, PrivilegeTaint::Public) {
        return Ok(());
    }
    if manifest.allowed.contains(provider) {
        Ok(())
    } else {
        Err(PrivilegeError::ProviderNotAllowed(provider.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(ids: &[&str]) -> PrivilegeManifest {
        PrivilegeManifest::new(ids.iter().copied().map(ProviderId::new))
    }

    #[test]
    fn confidential_to_unlisted_provider_refused() {
        let m = manifest(&["anthropic-bring-your-own"]);
        let r = check_send(
            &m,
            &ProviderId::new("public-openai"),
            PrivilegeTaint::ClientConfidential,
        );
        assert!(matches!(r, Err(PrivilegeError::ProviderNotAllowed(_))));
    }

    #[test]
    fn confidential_to_listed_provider_ok() {
        let m = manifest(&["anthropic-bring-your-own"]);
        assert!(
            check_send(
                &m,
                &ProviderId::new("anthropic-bring-your-own"),
                PrivilegeTaint::ClientConfidential,
            )
            .is_ok()
        );
    }

    #[test]
    fn public_taint_bypasses_manifest() {
        let m = manifest(&[]);
        assert!(check_send(&m, &ProviderId::new("anything"), PrivilegeTaint::Public).is_ok());
    }

    #[test]
    fn default_taint_is_confidential() {
        assert_eq!(
            PrivilegeTaint::default(),
            PrivilegeTaint::ClientConfidential
        );
    }
}
