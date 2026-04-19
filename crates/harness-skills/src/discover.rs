use crate::parse::{ParsedSkill, parse_skill_md};
use crate::{Catalog, Skill, SkillLayout, SkillScope};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub project_root: PathBuf,
    pub home: PathBuf,
    pub max_depth: usize,
    pub max_entries: usize,
}

impl DiscoveryConfig {
    pub fn from_env() -> std::io::Result<Self> {
        let project_root = std::env::current_dir()?;
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        Ok(Self {
            project_root,
            home,
            max_depth: 4,
            max_entries: 2000,
        })
    }
}

#[must_use]
pub fn discover(cfg: &DiscoveryConfig) -> Catalog {
    let mut catalog = Catalog::default();

    for (scope, layout, root) in search_paths(cfg) {
        if !root.is_dir() {
            continue;
        }
        scan_one_root(
            &root,
            scope,
            layout,
            cfg.max_depth,
            cfg.max_entries,
            &mut catalog,
        );
    }

    catalog
}

fn search_paths(cfg: &DiscoveryConfig) -> Vec<(SkillScope, SkillLayout, PathBuf)> {
    vec![
        (
            SkillScope::Project,
            SkillLayout::Native,
            cfg.project_root.join(".harness").join("skills"),
        ),
        (
            SkillScope::Project,
            SkillLayout::Std,
            cfg.project_root.join(".agents").join("skills"),
        ),
        (
            SkillScope::Project,
            SkillLayout::Claude,
            cfg.project_root.join(".claude").join("skills"),
        ),
        (
            SkillScope::User,
            SkillLayout::Native,
            cfg.home.join(".harness").join("skills"),
        ),
        (
            SkillScope::User,
            SkillLayout::Std,
            cfg.home.join(".agents").join("skills"),
        ),
        (
            SkillScope::User,
            SkillLayout::Claude,
            cfg.home.join(".claude").join("skills"),
        ),
    ]
}

fn scan_one_root(
    root: &Path,
    scope: SkillScope,
    layout: SkillLayout,
    max_depth: usize,
    max_entries: usize,
    catalog: &mut Catalog,
) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut scanned = 0usize;

    while let Some((dir, depth)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        let read = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!(%e, path = %dir.display(), "skill scan: read_dir failed");
                continue;
            }
        };
        for entry in read {
            if scanned >= max_entries {
                tracing::warn!(
                    root = %root.display(),
                    "skill scan: hit max_entries limit"
                );
                return;
            }
            scanned += 1;
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if should_skip_dirname(&name) {
                continue;
            }
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.is_file() {
                    load_skill_from(&skill_md, &path, scope, layout, catalog);
                    continue;
                }
                stack.push((path, depth + 1));
            }
        }
    }
}

fn load_skill_from(
    skill_md: &Path,
    dir: &Path,
    scope: SkillScope,
    layout: SkillLayout,
    catalog: &mut Catalog,
) {
    let raw = match fs::read_to_string(skill_md) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                path = %skill_md.display(),
                %e,
                "skill: read failed"
            );
            return;
        }
    };
    let mut parsed = match parse_skill_md(&raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                path = %skill_md.display(),
                %e,
                "skill: parse failed — dropping"
            );
            return;
        }
    };

    let dir_name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if parsed.name.is_empty() {
        parsed.name.clone_from(&dir_name);
    } else if parsed.name != dir_name {
        tracing::warn!(
            path = %skill_md.display(),
            frontmatter_name = %parsed.name,
            dir_name = %dir_name,
            "skill: frontmatter name does not match directory name"
        );
    }
    for w in &parsed.warnings {
        tracing::warn!(path = %skill_md.display(), warning = %w, "skill: warning");
    }

    let ParsedSkill {
        name,
        description,
        license,
        compatibility,
        allowed_tools,
        metadata,
        ..
    } = parsed;

    let skill = Skill {
        name,
        description,
        location: skill_md.to_path_buf(),
        license,
        compatibility,
        allowed_tools,
        metadata,
        scope,
        layout,
    };
    catalog.insert(skill);
}

fn should_skip_dirname(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target" | ".DS_Store")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_skill(dir: &Path, frontmatter: &str, body: &str) {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join("SKILL.md");
        fs::write(path, format!("---\n{frontmatter}\n---\n{body}\n")).unwrap();
    }

    #[test]
    fn discovers_a_single_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        write_skill(
            &root.join(".harness/skills/echo"),
            "name: echo\ndescription: prints hello",
            "Say hello.",
        );

        let cfg = DiscoveryConfig {
            project_root: root,
            home: tmp.path().join("fake-home"),
            max_depth: 4,
            max_entries: 2000,
        };
        let cat = discover(&cfg);
        let s = cat.get("echo").expect("echo discovered");
        assert_eq!(s.description, "prints hello");
        assert_eq!(s.scope, SkillScope::Project);
        assert_eq!(s.layout, SkillLayout::Native);
    }

    #[test]
    fn project_wins_over_user() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("proj");
        let home = tmp.path().join("home");
        write_skill(
            &project.join(".agents/skills/same"),
            "name: same\ndescription: project version",
            "p",
        );
        write_skill(
            &home.join(".agents/skills/same"),
            "name: same\ndescription: user version",
            "u",
        );
        let cat = discover(&DiscoveryConfig {
            project_root: project,
            home,
            max_depth: 4,
            max_entries: 2000,
        });
        let s = cat.get("same").unwrap();
        assert_eq!(s.description, "project version");
        assert_eq!(s.scope, SkillScope::Project);
    }

    #[test]
    fn native_wins_over_std_wins_over_claude() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        write_skill(
            &home.join(".harness/skills/triple"),
            "name: triple\ndescription: native",
            "",
        );
        write_skill(
            &home.join(".agents/skills/triple"),
            "name: triple\ndescription: std",
            "",
        );
        write_skill(
            &home.join(".claude/skills/triple"),
            "name: triple\ndescription: claude",
            "",
        );
        let cat = discover(&DiscoveryConfig {
            project_root: tmp.path().join("empty-project"),
            home,
            max_depth: 4,
            max_entries: 2000,
        });
        let s = cat.get("triple").unwrap();
        assert_eq!(s.description, "native");
        assert_eq!(s.layout, SkillLayout::Native);
    }

    #[test]
    fn skips_invalid_skill_but_keeps_others() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        write_skill(&root.join(".agents/skills/broken"), "name: broken", "");
        write_skill(
            &root.join(".agents/skills/fine"),
            "name: fine\ndescription: ok",
            "",
        );
        let cat = discover(&DiscoveryConfig {
            project_root: root,
            home: tmp.path().join("home"),
            max_depth: 4,
            max_entries: 2000,
        });
        assert!(cat.get("broken").is_none());
        assert!(cat.get("fine").is_some());
    }

    #[test]
    fn nested_skill_directory_is_found() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        write_skill(
            &root.join(".agents/skills/category/nested"),
            "name: nested\ndescription: d",
            "",
        );
        let cat = discover(&DiscoveryConfig {
            project_root: root,
            home: tmp.path().join("home"),
            max_depth: 4,
            max_entries: 2000,
        });
        assert!(cat.get("nested").is_some());
    }
}
