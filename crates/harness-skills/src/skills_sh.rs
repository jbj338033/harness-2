// IMPLEMENTS: D-158, D-204
//! `skills.sh` compatibility — Anthropic / OpenAI / Cursor / Vercel all
//! converged on a "directory of shell scripts with header comments"
//! contract. We ingest one of those directories and emit a Harness-native
//! `SKILL.md` for each entry so the rest of the discovery pipeline can
//! stay agnostic.
//!
//! A skills.sh entry typically looks like:
//!
//! ```sh
//! #!/usr/bin/env bash
//! # name: deploy-staging
//! # description: Push the current branch to the staging environment.
//! # license: MIT
//! cd "$(git rev-parse --show-toplevel)"
//! ./scripts/deploy.sh staging
//! ```
//!
//! The header lines (`# key: value` until the first non-`#` line) become
//! SKILL.md frontmatter; the rest of the script is preserved as the
//! body — Harness can either run it via `bash skill.sh` or expose it to
//! the LLM as documentation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("source {0} is not a directory")]
    NotADirectory(PathBuf),
    #[error("script {path} is missing the required `name` header")]
    MissingName { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedScript {
    pub name: String,
    pub description: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub written: Vec<PathBuf>,
    pub skipped: Vec<(PathBuf, String)>,
}

/// Walk a skills.sh directory and produce one parsed entry per script.
pub fn scan_dir(source: &Path) -> Result<Vec<ParsedScript>, ImportError> {
    if !source.is_dir() {
        return Err(ImportError::NotADirectory(source.to_path_buf()));
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(source)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if !is_script(&path) {
            continue;
        }
        let body = std::fs::read_to_string(&path)?;
        let parsed = parse_script(&body, &path)?;
        out.push(parsed);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn is_script(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("sh") | Some("bash") | None
    )
}

pub fn parse_script(body: &str, source: &Path) -> Result<ParsedScript, ImportError> {
    let mut headers: BTreeMap<String, String> = BTreeMap::new();
    let mut body_start = 0usize;
    for (idx, line) in body.lines().enumerate() {
        if idx == 0 && line.starts_with("#!") {
            body_start = body
                .lines()
                .take(idx + 1)
                .map(|l| l.len() + 1)
                .sum::<usize>();
            continue;
        }
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('#') {
            let kv = rest.trim_start();
            if let Some((k, v)) = kv.split_once(':') {
                let key = k.trim().to_ascii_lowercase();
                let value = v.trim().to_string();
                if !key.is_empty() {
                    headers.insert(key, value);
                }
            }
            body_start = body
                .lines()
                .take(idx + 1)
                .map(|l| l.len() + 1)
                .sum::<usize>();
        } else if !trimmed.is_empty() {
            break;
        }
    }
    let name = headers
        .get("name")
        .cloned()
        .or_else(|| {
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .ok_or_else(|| ImportError::MissingName {
            path: source.to_path_buf(),
        })?;
    let description = headers.get("description").cloned().unwrap_or_default();
    let body_text = body
        .get(body_start.min(body.len())..)
        .unwrap_or("")
        .trim_start()
        .to_string();
    Ok(ParsedScript {
        name,
        description,
        headers,
        body: body_text,
        source_path: source.to_path_buf(),
    })
}

/// Render a parsed script back as a SKILL.md. Frontmatter is the YAML-ish
/// `key: value` block harness-skills already parses; the script body lives
/// under a fenced `bash` code block so the LLM can read it.
#[must_use]
pub fn render_skill_md(script: &ParsedScript) -> String {
    let mut out = String::with_capacity(script.body.len() + 256);
    out.push_str("---\n");
    out.push_str(&format!("name: {}\n", script.name));
    if !script.description.is_empty() {
        out.push_str(&format!("description: {}\n", script.description));
    }
    for (k, v) in &script.headers {
        if k == "name" || k == "description" {
            continue;
        }
        out.push_str(&format!("{k}: {v}\n"));
    }
    out.push_str("source: skills.sh\n");
    out.push_str("---\n\n");
    if !script.body.is_empty() {
        out.push_str("```bash\n");
        out.push_str(&script.body);
        if !script.body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```\n");
    }
    out
}

/// Walk `source`, render each script as a SKILL.md, and write them under
/// `dest_root/<name>/SKILL.md`. Existing entries are skipped (no
/// overwrite) so re-running the import is safe.
pub fn import_from_dir(source: &Path, dest_root: &Path) -> Result<ImportReport, ImportError> {
    let scripts = scan_dir(source)?;
    let mut report = ImportReport::default();
    for script in scripts {
        let dir = dest_root.join(&script.name);
        let target = dir.join("SKILL.md");
        if target.exists() {
            report.skipped.push((target, "already imported".into()));
            continue;
        }
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&target, render_skill_md(&script))?;
        report.written.push(target);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn parse_extracts_name_and_description_from_headers() {
        let body = "#!/usr/bin/env bash\n# name: deploy\n# description: ship it\necho ok\n";
        let s = parse_script(body, Path::new("deploy.sh")).unwrap();
        assert_eq!(s.name, "deploy");
        assert_eq!(s.description, "ship it");
        assert_eq!(s.body.trim(), "echo ok");
    }

    #[test]
    fn parse_falls_back_to_filename_when_name_header_missing() {
        let body = "#!/usr/bin/env bash\necho hi\n";
        let s = parse_script(body, Path::new("/tmp/build-staging.sh")).unwrap();
        assert_eq!(s.name, "build-staging");
    }

    #[test]
    fn parse_collects_extra_headers() {
        let body = "# name: x\n# license: MIT\n# version: 1.2\nbody\n";
        let s = parse_script(body, Path::new("x.sh")).unwrap();
        assert_eq!(s.headers.get("license").map(String::as_str), Some("MIT"));
        assert_eq!(s.headers.get("version").map(String::as_str), Some("1.2"));
    }

    #[test]
    fn render_skill_md_contains_yaml_frontmatter_and_bash_block() {
        let s = ParsedScript {
            name: "noop".into(),
            description: "nothing".into(),
            headers: BTreeMap::new(),
            body: "echo hi".into(),
            source_path: PathBuf::from("/tmp/noop.sh"),
        };
        let md = render_skill_md(&s);
        assert!(md.starts_with("---\n"));
        assert!(md.contains("name: noop"));
        assert!(md.contains("description: nothing"));
        assert!(md.contains("```bash\n"));
        assert!(md.contains("echo hi"));
    }

    #[test]
    fn scan_dir_returns_sorted_entries() {
        let dir = TempDir::new().unwrap();
        write(&dir.path().join("b.sh"), "# name: b\n# description: B\n");
        write(&dir.path().join("a.sh"), "# name: a\n# description: A\n");
        let scripts = scan_dir(dir.path()).unwrap();
        assert_eq!(
            scripts.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    #[test]
    fn import_writes_skill_md_per_script_and_skips_existing() {
        let src = TempDir::new().unwrap();
        write(
            &src.path().join("s.sh"),
            "# name: s\n# description: hi\necho 1\n",
        );
        let dest = TempDir::new().unwrap();
        let r1 = import_from_dir(src.path(), dest.path()).unwrap();
        assert_eq!(r1.written.len(), 1);
        assert!(dest.path().join("s").join("SKILL.md").exists());
        let r2 = import_from_dir(src.path(), dest.path()).unwrap();
        assert!(r2.written.is_empty());
        assert_eq!(r2.skipped.len(), 1);
    }

    #[test]
    fn import_rejects_non_directory_source() {
        let dest = TempDir::new().unwrap();
        let err = import_from_dir(Path::new("/this/is/not/here"), dest.path()).unwrap_err();
        assert!(matches!(err, ImportError::NotADirectory(_)), "got {err:?}");
    }
}
