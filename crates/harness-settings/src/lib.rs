// IMPLEMENTS: D-179
//! Claude-compatible `.claude/settings.json` hierarchy loader.
//!
//! Resolution order (highest precedence first):
//!   1. `<project>/.claude/settings.local.json`  — gitignored per-clone
//!   2. `<project>/.claude/settings.json`        — checked into the repo
//!   3. `~/.claude/settings.json`                — user global
//!   4. `<project>/.claude/hooks.json`           — plugin bundle (D-179)
//!
//! Higher tiers deep-merge over lower ones: object values are merged key
//! by key, every other JSON kind is replaced. The `hooks.json` tier sits
//! at the bottom so a plugin can ship sane defaults but the project /
//! user can always override.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("io reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadedSource {
    pub path: PathBuf,
    pub value: Value,
}

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub merged: Value,
    /// Layers in resolution order — highest precedence first.
    pub sources: Vec<LoadedSource>,
}

/// Load and merge the four-tier settings hierarchy. Missing files are
/// silently skipped (the daemon should run with no settings at all).
pub fn load(project_root: &Path, home: &Path) -> Result<Settings, SettingsError> {
    let candidates = [
        project_root.join(".claude").join("settings.local.json"),
        project_root.join(".claude").join("settings.json"),
        home.join(".claude").join("settings.json"),
        project_root.join(".claude").join("hooks.json"),
    ];

    let mut sources: Vec<LoadedSource> = Vec::new();
    for path in candidates {
        if let Some(loaded) = read_optional(&path)? {
            sources.push(loaded);
        }
    }

    // Merge from lowest precedence (last in `sources`) up so higher tiers
    // win on key conflicts.
    let mut merged = Value::Object(Map::new());
    for src in sources.iter().rev() {
        merged = deep_merge(merged, src.value.clone());
    }

    Ok(Settings { merged, sources })
}

fn read_optional(path: &Path) -> Result<Option<LoadedSource>, SettingsError> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|source| SettingsError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|source| SettingsError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(LoadedSource {
        path: path.to_path_buf(),
        value,
    }))
}

/// Deep merge — `over` wins on conflicts, objects merge key by key, every
/// other shape replaces wholesale.
#[must_use]
pub fn deep_merge(base: Value, over: Value) -> Value {
    match (base, over) {
        (Value::Object(mut b), Value::Object(o)) => {
            for (k, v) in o {
                let base_v = b.remove(&k).unwrap_or(Value::Null);
                b.insert(k, deep_merge(base_v, v));
            }
            Value::Object(b)
        }
        (_, over) => over,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn missing_files_yield_empty_settings() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let s = load(project.path(), home.path()).unwrap();
        assert_eq!(s.merged, json!({}));
        assert!(s.sources.is_empty());
    }

    #[test]
    fn project_overrides_user() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        write(
            &home.path().join(".claude/settings.json"),
            r#"{"theme":"light","tools":{"shell":"deny"}}"#,
        );
        write(
            &project.path().join(".claude/settings.json"),
            r#"{"theme":"dark"}"#,
        );
        let s = load(project.path(), home.path()).unwrap();
        assert_eq!(s.merged["theme"], "dark");
        // Tools come from the user tier — project didn't set them.
        assert_eq!(s.merged["tools"]["shell"], "deny");
    }

    #[test]
    fn local_overrides_project_overrides_user() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        write(
            &home.path().join(".claude/settings.json"),
            r#"{"editor":{"font":"sans","size":12}}"#,
        );
        write(
            &project.path().join(".claude/settings.json"),
            r#"{"editor":{"font":"mono"}}"#,
        );
        write(
            &project.path().join(".claude/settings.local.json"),
            r#"{"editor":{"size":14}}"#,
        );
        let s = load(project.path(), home.path()).unwrap();
        assert_eq!(s.merged["editor"]["font"], "mono");
        assert_eq!(s.merged["editor"]["size"], 14);
    }

    #[test]
    fn hooks_json_is_lowest_precedence() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        write(
            &project.path().join(".claude/hooks.json"),
            r#"{"plugin":{"x":"plugin-default"}}"#,
        );
        write(
            &project.path().join(".claude/settings.json"),
            r#"{"plugin":{"x":"project-override"}}"#,
        );
        let s = load(project.path(), home.path()).unwrap();
        assert_eq!(s.merged["plugin"]["x"], "project-override");
    }

    #[test]
    fn deep_merge_objects_replace_non_objects() {
        let base = json!({ "k": [1, 2, 3] });
        let over = json!({ "k": [9] });
        assert_eq!(deep_merge(base, over), json!({"k":[9]}));
    }

    #[test]
    fn deep_merge_objects_recursively() {
        let base = json!({"a":{"b":1,"c":2},"d":3});
        let over = json!({"a":{"b":99,"e":4}});
        assert_eq!(
            deep_merge(base, over),
            json!({"a":{"b":99,"c":2,"e":4},"d":3})
        );
    }

    #[test]
    fn parse_error_surfaces_path() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        write(
            &project.path().join(".claude/settings.json"),
            "{ not valid json",
        );
        let err = load(project.path(), home.path()).unwrap_err();
        match err {
            SettingsError::Parse { path, .. } => {
                assert!(path.ends_with(".claude/settings.json"));
            }
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn sources_field_records_load_order() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        write(&project.path().join(".claude/settings.json"), "{}");
        write(&home.path().join(".claude/settings.json"), "{}");
        let s = load(project.path(), home.path()).unwrap();
        assert_eq!(s.sources.len(), 2);
        assert!(s.sources[0].path.ends_with(".claude/settings.json"));
        assert!(s.sources[0].path.starts_with(project.path()));
        assert!(s.sources[1].path.starts_with(home.path()));
    }
}
