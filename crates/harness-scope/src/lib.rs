// IMPLEMENTS: D-327
use harness_auth::{PrivateKey, PublicKey, SignatureBytes};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum ScopeError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("signature verification failed")]
    BadSignature,
    #[error("invalid url: {0}")]
    BadUrl(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(String),
}

/// What the user explicitly authorised — DNS hostnames, HTTP URL prefixes,
/// and shell program names. Anything not in here is denied (fail-closed).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScopePolicy {
    #[serde(default)]
    pub allowed_dns: Vec<String>,
    #[serde(default)]
    pub allowed_http_prefixes: Vec<String>,
    #[serde(default)]
    pub allowed_shell_programs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeFile {
    pub policy: ScopePolicy,
    pub public_key: PublicKey,
    pub signature: SignatureBytes,
}

impl ScopeFile {
    pub fn sign(sk: &PrivateKey, policy: ScopePolicy) -> Result<Self, ScopeError> {
        let bytes = canonical(&policy)?;
        let signature = sk.sign(&bytes);
        let public_key = sk.public();
        Ok(Self {
            policy,
            public_key,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), ScopeError> {
        let bytes = canonical(&self.policy)?;
        self.public_key
            .verify(&bytes, &self.signature)
            .map_err(|_| ScopeError::BadSignature)
    }

    pub fn write(&self, path: &Path) -> Result<(), ScopeError> {
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, ScopeError> {
        let bytes = std::fs::read(path)?;
        let file: Self = serde_json::from_slice(&bytes)?;
        file.verify()?;
        Ok(file)
    }
}

/// Fail-closed enforcer — every check defaults to Deny unless the policy
/// explicitly allows it. The empty-policy case (no rules at all) denies
/// everything; that is the safe default for a fresh install.
#[derive(Debug, Clone)]
pub struct Enforcer {
    policy: ScopePolicy,
}

impl Enforcer {
    #[must_use]
    pub fn new(policy: ScopePolicy) -> Self {
        Self { policy }
    }

    #[must_use]
    pub fn deny_all() -> Self {
        Self::new(ScopePolicy::default())
    }

    #[must_use]
    pub fn check_dns(&self, host: &str) -> Verdict {
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() {
            return Verdict::Deny("empty hostname".into());
        }
        if self
            .policy
            .allowed_dns
            .iter()
            .any(|allowed| matches_dns(allowed, &host))
        {
            Verdict::Allow
        } else {
            Verdict::Deny(format!("dns {host} not in scope"))
        }
    }

    pub fn check_http(&self, url: &str) -> Result<Verdict, ScopeError> {
        let parsed = Url::parse(url).map_err(|e| ScopeError::BadUrl(e.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Ok(Verdict::Deny(format!(
                "scheme {} not allowed",
                parsed.scheme()
            )));
        }
        if let Some(host) = parsed.host_str() {
            let dns = self.check_dns(host);
            if matches!(dns, Verdict::Deny(_)) {
                return Ok(dns);
            }
        } else {
            return Ok(Verdict::Deny("url has no host".into()));
        }
        if self
            .policy
            .allowed_http_prefixes
            .iter()
            .any(|prefix| url.starts_with(prefix.as_str()))
        {
            Ok(Verdict::Allow)
        } else if self.policy.allowed_http_prefixes.is_empty() {
            // No prefix list — DNS allow is enough (mirrors curl-style scoping).
            Ok(Verdict::Allow)
        } else {
            Ok(Verdict::Deny(format!("url {url} not in scope")))
        }
    }

    #[must_use]
    pub fn check_shell(&self, program: &str) -> Verdict {
        let bare = program.rsplit('/').next().unwrap_or(program);
        if self
            .policy
            .allowed_shell_programs
            .iter()
            .any(|p| p == bare || p == program)
        {
            Verdict::Allow
        } else {
            Verdict::Deny(format!("program {bare} not in scope"))
        }
    }
}

fn canonical(policy: &ScopePolicy) -> Result<Vec<u8>, ScopeError> {
    Ok(serde_json::to_vec(policy)?)
}

fn matches_dns(rule: &str, host: &str) -> bool {
    let rule = rule.trim().to_ascii_lowercase();
    if rule == host {
        return true;
    }
    if let Some(suffix) = rule.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_auth::generate_keypair;
    use tempfile::TempDir;

    fn policy() -> ScopePolicy {
        ScopePolicy {
            allowed_dns: vec!["api.openai.com".into(), "*.anthropic.com".into()],
            allowed_http_prefixes: vec!["https://api.openai.com/v1/".into()],
            allowed_shell_programs: vec!["cargo".into(), "git".into()],
        }
    }

    #[test]
    fn deny_all_blocks_everything() {
        let e = Enforcer::deny_all();
        assert!(matches!(e.check_dns("example.com"), Verdict::Deny(_)));
        assert!(matches!(
            e.check_http("https://example.com/").unwrap(),
            Verdict::Deny(_)
        ));
        assert!(matches!(e.check_shell("ls"), Verdict::Deny(_)));
    }

    #[test]
    fn dns_allows_exact_match_and_wildcard() {
        let e = Enforcer::new(policy());
        assert!(matches!(e.check_dns("api.openai.com"), Verdict::Allow));
        assert!(matches!(e.check_dns("api.anthropic.com"), Verdict::Allow));
        assert!(matches!(
            e.check_dns("private.api.anthropic.com"),
            Verdict::Allow
        ));
        assert!(matches!(e.check_dns("evil.com"), Verdict::Deny(_)));
    }

    #[test]
    fn http_requires_dns_and_prefix() {
        let e = Enforcer::new(policy());
        assert!(matches!(
            e.check_http("https://api.openai.com/v1/chat/completions")
                .unwrap(),
            Verdict::Allow
        ));
        // DNS allowed but prefix mismatch
        assert!(matches!(
            e.check_http("https://api.openai.com/admin/").unwrap(),
            Verdict::Deny(_)
        ));
        // DNS not allowed
        assert!(matches!(
            e.check_http("https://evil.com/").unwrap(),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn http_rejects_non_http_scheme() {
        let e = Enforcer::new(policy());
        assert!(matches!(
            e.check_http("file:///etc/passwd").unwrap(),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn shell_matches_bare_program_name() {
        let e = Enforcer::new(policy());
        assert!(matches!(e.check_shell("cargo"), Verdict::Allow));
        assert!(matches!(e.check_shell("/usr/bin/git"), Verdict::Allow));
        assert!(matches!(e.check_shell("rm"), Verdict::Deny(_)));
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let (sk, _) = generate_keypair();
        let file = ScopeFile::sign(&sk, policy()).unwrap();
        file.verify().unwrap();
    }

    #[test]
    fn tampered_policy_fails_verification() {
        let (sk, _) = generate_keypair();
        let mut file = ScopeFile::sign(&sk, policy()).unwrap();
        file.policy.allowed_dns.push("attacker.com".into());
        let err = file.verify().unwrap_err();
        assert!(matches!(err, ScopeError::BadSignature), "got {err:?}");
    }

    #[test]
    fn write_then_load_roundtrips() {
        let (sk, _) = generate_keypair();
        let file = ScopeFile::sign(&sk, policy()).unwrap();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("scope.json");
        file.write(&path).unwrap();
        let loaded = ScopeFile::load(&path).unwrap();
        assert_eq!(loaded.policy.allowed_dns, file.policy.allowed_dns);
    }

    #[test]
    fn load_refuses_tampered_file() {
        let (sk, _) = generate_keypair();
        let file = ScopeFile::sign(&sk, policy()).unwrap();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("scope.json");
        file.write(&path).unwrap();

        let mut on_disk: ScopeFile =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        on_disk.policy.allowed_shell_programs.push("rm".into());
        std::fs::write(&path, serde_json::to_vec(&on_disk).unwrap()).unwrap();

        let err = ScopeFile::load(&path).unwrap_err();
        assert!(matches!(err, ScopeError::BadSignature));
    }
}
