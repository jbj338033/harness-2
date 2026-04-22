// IMPLEMENTS: D-315, D-316
use serde::{Deserialize, Serialize};

/// Identity card every protocol surface emits — used by `harness protocol check`
/// to detect drift between the version a remote speaker reports and the one
/// pinned in our manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolIdentity {
    /// Wire-level name, lowercase. e.g. "mcp", "acp", "a2a", "agent_trace".
    pub name: String,
    /// Pinned version harness was built against.
    pub current_version: String,
    /// blake3 hex of the pinned spec / wire schema bytes — drift if a remote
    /// reports the same version but a different schema_hash.
    pub schema_hash: String,
}

/// All wire protocols implement this so the daemon, the CLI, and the drift
/// detector can introspect them uniformly.
pub trait ProtocolAdapter: Send + Sync {
    fn identity(&self) -> ProtocolIdentity;
}

#[derive(Debug, Clone, Default)]
pub struct McpAdapter;

impl ProtocolAdapter for McpAdapter {
    fn identity(&self) -> ProtocolIdentity {
        ProtocolIdentity {
            name: "mcp".into(),
            current_version: harness_mcp::MCP_PROTOCOL_VERSION.into(),
            schema_hash: schema_hash_hex(MCP_PINNED_SCHEMA_BYTES),
        }
    }
}

const MCP_PINNED_SCHEMA_BYTES: &[u8] = b"mcp/2025-06-18/initialize+tools+content+progress+roots";

fn schema_hash_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().chars().take(16).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftStatus {
    InSync,
    VersionDrift {
        expected: String,
        actual: String,
    },
    SchemaDrift {
        version: String,
        expected_hash: String,
        actual_hash: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub name: String,
    pub expected_version: String,
    pub expected_schema_hash: String,
}

pub fn check(
    manifest: &[ManifestEntry],
    adapters: &[&dyn ProtocolAdapter],
) -> Vec<(String, DriftStatus)> {
    let mut out = Vec::with_capacity(adapters.len());
    for a in adapters {
        let id = a.identity();
        let entry = manifest.iter().find(|m| m.name == id.name);
        let status = match entry {
            None => DriftStatus::VersionDrift {
                expected: "(not in manifest)".into(),
                actual: id.current_version.clone(),
            },
            Some(m) if m.expected_version != id.current_version => DriftStatus::VersionDrift {
                expected: m.expected_version.clone(),
                actual: id.current_version.clone(),
            },
            Some(m) if m.expected_schema_hash != id.schema_hash => DriftStatus::SchemaDrift {
                version: id.current_version.clone(),
                expected_hash: m.expected_schema_hash.clone(),
                actual_hash: id.schema_hash.clone(),
            },
            _ => DriftStatus::InSync,
        };
        out.push((id.name, status));
    }
    out
}

#[must_use]
pub fn manifest() -> Vec<ManifestEntry> {
    let adapters: &[&dyn ProtocolAdapter] = &[&McpAdapter];
    adapters
        .iter()
        .map(|a| {
            let id = a.identity();
            ManifestEntry {
                name: id.name,
                expected_version: id.current_version,
                expected_schema_hash: id.schema_hash,
            }
        })
        .collect()
}

#[must_use]
pub fn builtin_adapters() -> Vec<Box<dyn ProtocolAdapter>> {
    vec![Box::new(McpAdapter)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_identity_pins_to_2025_06_18() {
        let id = McpAdapter.identity();
        assert_eq!(id.name, "mcp");
        assert_eq!(id.current_version, "2025-06-18");
        assert_eq!(id.schema_hash.len(), 16);
    }

    #[test]
    fn check_in_sync_when_manifest_matches() {
        let m = manifest();
        let adapters = builtin_adapters();
        let refs: Vec<&dyn ProtocolAdapter> = adapters.iter().map(|a| a.as_ref()).collect();
        let res = check(&m, &refs);
        for (name, status) in res {
            assert_eq!(status, DriftStatus::InSync, "{name}");
        }
    }

    #[test]
    fn check_reports_version_drift_when_manifest_pins_old_version() {
        let stale = vec![ManifestEntry {
            name: "mcp".into(),
            expected_version: "2024-11-05".into(),
            expected_schema_hash: McpAdapter.identity().schema_hash,
        }];
        let adapter = McpAdapter;
        let res = check(&stale, &[&adapter]);
        assert!(matches!(
            res[0].1,
            DriftStatus::VersionDrift { ref expected, .. } if expected == "2024-11-05"
        ));
    }

    #[test]
    fn check_reports_schema_drift_when_only_hash_differs() {
        let stale = vec![ManifestEntry {
            name: "mcp".into(),
            expected_version: harness_mcp::MCP_PROTOCOL_VERSION.into(),
            expected_schema_hash: "deadbeefdeadbeef".into(),
        }];
        let adapter = McpAdapter;
        let res = check(&stale, &[&adapter]);
        assert!(matches!(res[0].1, DriftStatus::SchemaDrift { .. }));
    }

    #[test]
    fn check_reports_unknown_protocol_when_missing_from_manifest() {
        let empty: Vec<ManifestEntry> = Vec::new();
        let adapter = McpAdapter;
        let res = check(&empty, &[&adapter]);
        assert!(matches!(
            res[0].1,
            DriftStatus::VersionDrift { ref expected, .. } if expected == "(not in manifest)"
        ));
    }
}
