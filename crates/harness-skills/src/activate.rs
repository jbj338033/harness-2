use crate::Skill;
use crate::parse::{ParseError, parse_skill_md};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub name: String,
    pub body: String,
    pub directory: PathBuf,
    pub resources: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ActivateError {
    #[error("read SKILL.md: {0}")]
    Read(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

pub fn activate(skill: &Skill) -> Result<Activation, ActivateError> {
    let raw = fs::read_to_string(&skill.location)?;
    let parsed = parse_skill_md(&raw)?;
    let dir = skill.directory().to_path_buf();
    let resources = enumerate_resources(&dir);
    Ok(Activation {
        name: skill.name.clone(),
        body: parsed.body,
        directory: dir,
        resources,
    })
}

fn enumerate_resources(dir: &Path) -> Vec<String> {
    const SUBDIRS: &[&str] = &["scripts", "references", "assets"];
    let mut out = Vec::new();
    for sub in SUBDIRS {
        walk_into(&dir.join(sub), dir, &mut out);
    }
    out.sort();
    out
}

fn walk_into(current: &Path, base: &Path, out: &mut Vec<String>) {
    let Ok(read) = fs::read_dir(current) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_into(&path, base, out);
        } else if path.is_file()
            && let Ok(rel) = path.strip_prefix(base)
        {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SkillLayout, SkillScope};
    use std::collections::BTreeMap;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    fn sample_skill(dir: &Path, name: &str) -> Skill {
        let skill_md = dir.join("SKILL.md");
        write(
            &skill_md,
            &format!("---\nname: {name}\ndescription: test\n---\nBody text.\n"),
        );
        Skill {
            name: name.into(),
            description: "test".into(),
            location: skill_md,
            license: None,
            compatibility: None,
            allowed_tools: None,
            metadata: BTreeMap::new(),
            scope: SkillScope::User,
            layout: SkillLayout::Std,
        }
    }

    #[test]
    fn activates_body() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("echo");
        let skill = sample_skill(&dir, "echo");
        let a = activate(&skill).unwrap();
        assert_eq!(a.name, "echo");
        assert!(a.body.contains("Body text."));
        assert_eq!(a.directory, dir);
    }

    #[test]
    fn enumerates_resources() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("pdf");
        let skill = sample_skill(&dir, "pdf");
        write(&dir.join("scripts/extract.py"), "# py");
        write(&dir.join("references/spec.md"), "# md");
        write(&dir.join("assets/template.html"), "<p/>");
        write(&dir.join("scripts/helpers/util.sh"), "# sh");

        let a = activate(&skill).unwrap();
        assert!(a.resources.contains(&"scripts/extract.py".to_string()));
        assert!(a.resources.contains(&"scripts/helpers/util.sh".to_string()));
        assert!(a.resources.contains(&"references/spec.md".to_string()));
        assert!(a.resources.contains(&"assets/template.html".to_string()));
        let mut sorted = a.resources.clone();
        sorted.sort();
        assert_eq!(a.resources, sorted);
    }

    #[test]
    fn missing_file_returns_read_error() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = Skill {
            name: "ghost".into(),
            description: "d".into(),
            location: tmp.path().join("ghost/SKILL.md"),
            license: None,
            compatibility: None,
            allowed_tools: None,
            metadata: BTreeMap::new(),
            scope: SkillScope::User,
            layout: SkillLayout::Std,
        };
        assert!(matches!(activate(&skill), Err(ActivateError::Read(_))));
    }
}
